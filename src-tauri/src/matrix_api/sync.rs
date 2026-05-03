use std::collections::{HashMap, HashSet};

use log::{info, trace, warn};
use ruma::events::presence::PresenceEventContent;
use ruma::presence::PresenceState;
use ruma::serde::Raw;
use serde_json::value::RawValue;
use shared::messages::UiMessage;
use shared::user_profile::{PresenceInfo, PresenceStatus};
use tauri::{AppHandle, Emitter};
use tauri_plugin_http::reqwest::{self, Client};

use crate::frontend::{send_member_update, send_sidebar_update};
use crate::matrix_api::rooms::backfill_gap;
use crate::storage::members::MemberRow;
use crate::storage::messages::MessageRow;
use crate::storage::receipts::ReadReceiptRow;
use crate::storage::rooms::{SpaceChildRow, SpaceParentRow};
use crate::{AppState, TauriError, construct_url, matrix_api::crypto};

use ruma::OwnedRoomId;
use ruma::api::{IncomingResponse, client::sync::sync_events::v3::Response as SyncResponse};
use tokio_util::sync::CancellationToken;

async fn matrix_sync(
    access_token: &String,
    matrix_url: &String,
    since: Option<String>,
) -> Result<SyncResponse, TauriError> {
    let client = Client::new();

    let mut url = construct_url(vec![
        matrix_url,
        &"_matrix".to_string(),
        &"client".to_string(),
        &"v3".to_string(),
        &"sync".to_string(),
    ])?;

    let mut params = vec![("timeout", "30000".to_string())];

    if let Some(since_token) = since {
        params.push(("since", since_token));
    }

    // Increase timeline limit to 100 to avoid missing messages after being offline.
    // This is the most necessary change to fix missing messages after short periods of inactivity.
    params.push((
        "filter",
        "{\"room\":{\"timeline\":{\"limit\":100}}}".to_string(),
    ));

    url = reqwest::Url::parse_with_params(url.as_str(), &params)?;

    let res = client
        .get(url)
        .timeout(std::time::Duration::from_secs(35))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let status = res.status();
    let headers = res.headers().clone();
    let body_bytes = res
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    let mut builder = http::Response::builder().status(status);

    for (key, value) in headers.iter() {
        builder = builder.header(key, value);
    }

    let http_response = builder
        .body(body_bytes.to_vec())
        .map_err(|e| format!("Failed to build HTTP response: {e}"))?;

    match SyncResponse::try_from_http_response(http_response) {
        Ok(sync_response) => Ok(sync_response),
        Err(e) => Err(format!("Failed to parse sync response: {e}").into()),
    }
}

impl AppState {
    pub(crate) async fn start_sync(
        self: &std::sync::Arc<Self>,
        app_handle: &AppHandle,
        resync: bool,
    ) -> Result<(), TauriError> {
        let mut task_guard = self.sync_task.lock().await;
        if task_guard.is_some() {
            return Ok(());
        }

        let cancel = CancellationToken::new();
        {
            let mut cancel_guard = self.sync_cancel_token.lock().await;
            *cancel_guard = Some(cancel.clone());
        }

        let state = self.clone();
        let app_handle = app_handle.clone();
        let handle = tauri::async_runtime::spawn(async move {
            if let Err(e) = run_sync_loop(state, app_handle, resync).await {
                log::error!("Sync loop error: {:?}", e);
            }
        });

        *task_guard = Some(handle);
        Ok(())
    }

    pub(crate) async fn stop_sync(&self) -> Result<(), TauriError> {
        if let Some(cancel) = self.sync_cancel_token.lock().await.take() {
            cancel.cancel();
        }

        if let Some(handle) = self.sync_task.lock().await.take() {
            let _ = handle.await;
        }

        Ok(())
    }

    pub(crate) async fn restart_sync(
        self: &std::sync::Arc<Self>,
        handle: &AppHandle,
    ) -> Result<(), TauriError> {
        self.stop_sync().await?;
        self.start_sync(handle, true).await
    }
}

async fn run_sync_loop(
    state: std::sync::Arc<AppState>,
    handle: AppHandle,
    resync: bool,
) -> Result<(), TauriError> {
    let mut since = if resync {
        None
    } else {
        let guard = state.next_batch.read().await;
        guard.clone()
    };

    let cancel = {
        let cancel_guard = state.sync_cancel_token.lock().await;
        if let Some(cancel) = cancel_guard.as_ref() {
            cancel.clone()
        } else {
            return Err("Sync cancellation token not found".into());
        }
    };

    while !cancel.is_cancelled() {
        let access_token = state.check_token().await?;
        let matrix_url = {
            let guard = state.matrix_url.read().await;
            guard.as_ref().cloned().ok_or("Matrix URL not set")?
        };

        let sync_res = matrix_sync(&access_token, &matrix_url, since.clone()).await?;
        since = Some(sync_res.next_batch.clone());

        {
            let mut since_guard = state.next_batch.write().await;
            *since_guard = since.clone();
        }

        let olm_machine = {
            let guard = state.crypto_machine.lock().await;
            guard
                .as_ref()
                .cloned()
                .ok_or("Crypto machine not initialized")?
        };

        let res = crypto::process_sync_response(&olm_machine, sync_res, &access_token, &matrix_url)
            .await?;

        handle_sync_response(&state, &handle, res.clone()).await?;

        state.save_session().await?;
    }

    Ok(())
}

use crate::storage::{self, SyncChanges};

fn convert_presence(presence: PresenceEventContent) -> PresenceInfo {
    let status = match presence.presence {
        PresenceState::Offline => PresenceStatus::Offline,
        PresenceState::Online => PresenceStatus::Online,
        PresenceState::Unavailable => PresenceStatus::Unavailable,
        _ => PresenceStatus::Offline,
    };

    PresenceInfo {
        status: status,
        last_active_ago: presence.last_active_ago.map(|v| v.into()),
        status_msg: presence.status_msg,
    }
}

use ruma::api::client::sync::sync_events::v3::State as SyncState;
use ruma::events::{
    AnyGlobalAccountDataEvent, AnyStateEventContent, AnySyncEphemeralRoomEvent,
    AnySyncMessageLikeEvent, AnySyncStateEvent, AnySyncTimelineEvent,
};
async fn handle_sync_response(
    state: &std::sync::Arc<AppState>,
    handle: &AppHandle,
    response: SyncResponse,
) -> Result<(), TauriError> {
    let mut changes = SyncChanges::default();

    for raw_event in response.account_data.events {
        if let Err(e) = extract_account_data(&mut changes, raw_event) {
            log::error!("Error extracting account data: {:?}", e);
        }
    }

    info!("{:?}", response.presence);

    let payload: HashMap<String, PresenceInfo> = response
        .presence
        .events
        .into_iter()
        .filter_map(|ev| {
            if let Ok(data) = ev.deserialize() {
                Some((data.sender.to_string(), convert_presence(data.content)))
            } else {
                None
            }
        })
        .collect();

    handle.emit("presence_update", payload)?;

    let mut backfill_rooms = Vec::new();

    for (room_id, room) in response.rooms.join {
        changes.joined_rooms.push(room_id.clone());

        let update = changes.room_updates.entry(room_id.clone()).or_default();

        update.highlight_count = room
            .unread_notifications
            .highlight_count
            .map(|v| v.try_into().unwrap_or_default());
        update.notification_count = room
            .unread_notifications
            .notification_count
            .map(|v| v.try_into().unwrap_or_default());

        if let Some(prev_batch) = room.timeline.prev_batch {
            update.prev_batch = Some(prev_batch.clone());

            if room.timeline.limited {
                backfill_rooms.push((room_id.clone(), prev_batch));
            }
        }

        let room_state_events = match room.state {
            SyncState::After(v) => v.events,
            SyncState::Before(v) => v.events,
            _ => vec![],
        };

        for state_event in room_state_events {
            let data = match state_event.deserialize() {
                Ok(data) => data,
                Err(e) => {
                    warn!(
                        "Skipping malformed state event in room {}: {} | raw={}",
                        room_id,
                        e,
                        state_event.json().get()
                    );
                    continue;
                }
            };
            extract_state(&mut changes, &room_id, data.clone(), state_event.clone())?;
            // extract_special_state(&mut changes, &room_id, data, call_members)?;
        }

        for raw_event in room.timeline.events {
            let clone = raw_event.clone();
            let raw_json = clone.json();

            let data = match raw_event.deserialize() {
                Ok(data) => data,
                Err(e) => {
                    warn!(
                        "Skipping malformed timeline event in room {}: {} | raw={}",
                        room_id,
                        e,
                        raw_event.json().get()
                    );
                    continue;
                }
            };
            extract_timeline(&mut changes, &room_id, data, raw_json, raw_event.clone())?;
        }

        for raw_event in room.ephemeral.events {
            let data = match raw_event.deserialize() {
                Ok(data) => data,
                Err(e) => {
                    warn!(
                        "Skipping malformed ephemeral event in room {}: {} | raw={}",
                        room_id,
                        e,
                        raw_event.json().get()
                    );
                    continue;
                }
            };

            if let Err(e) = extract_ephemeral(&mut changes, &room_id, data) {
                warn!("Error extracting ephemeral event: {:?}", e);
            }
        }
    }

    let sidebar_needs_update = !changes.joined_rooms.is_empty()
        || changes.direct_rooms.is_some()
        || !changes.space_children.is_empty()
        || !changes.space_parents.is_empty()
        || changes.room_updates.values().any(|update| {
            update.name.is_some()
                || update.avatar_url.is_some()
                || update.room_type.is_some()
                || update.topic.is_some()
        });

    {
        let mut conn_guard = state.connection.lock().await;
        let conn = conn_guard
            .as_mut()
            .ok_or("Database connection not initialized")?;

        storage::apply_sync_changes(conn, changes.clone()).await?;

        let client_guard = state.client.read().await;
        let client = client_guard.as_ref().ok_or("Client info not initialized")?;

        if sidebar_needs_update {
            send_sidebar_update(conn, handle, &client.user_id)?;
        }
        if !changes.new_messages.is_empty() {
            let messages: HashMap<String, Vec<UiMessage>> = changes
                .new_messages
                .into_iter()
                .filter_map(|msg_row| {
                    let room_id = msg_row.room_id.clone();

                    let converted = msg_row.try_into().ok()?;
                    Some((room_id, converted))
                })
                .fold(HashMap::new(), |mut acc, (room_id, ui_msg)| {
                    acc.entry(room_id).or_default().push(ui_msg);
                    acc
                });

            crate::frontend::send_messages_update(handle, messages)?;
        }
    }

    for (room_id, prev_batch) in backfill_rooms {
        warn!(
            "Room {} timeline is limited! Starting backfill from {}",
            room_id, prev_batch
        );

        let state = state.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = backfill_gap(&state, room_id.to_string(), prev_batch).await {
                log::error!("Backfill error for room {}: {:?}", room_id, e);
            }
        });
    }

    send_member_update(handle, changes.member_updates.into())?;

    Ok(())
}

fn extract_ephemeral(
    changes: &mut SyncChanges,
    room_id: &OwnedRoomId,
    ev: AnySyncEphemeralRoomEvent,
) -> Result<(), TauriError> {
    match ev {
        AnySyncEphemeralRoomEvent::Receipt(ev) => {
            for (event_id, receipts_by_type) in ev.content.iter() {
                let event_id_str = event_id.to_string();

                for (receipt_type, users) in receipts_by_type.iter() {
                    let receipt_type_str = receipt_type.to_string();

                    for (user_id, receipt) in users.iter() {
                        let user_id_str = user_id.to_string();

                        let ts = receipt.ts.map(|t| t.as_secs().into()).unwrap_or_default();

                        changes.read_receipts.push(ReadReceiptRow {
                            room_id: room_id.to_string(),
                            user_id: user_id_str,
                            receipt_type: receipt_type_str.clone(),
                            event_id: event_id_str.clone(),
                            ts,
                        });
                    }
                }
            }
        }
        _ => (),
    }

    Ok(())
}

fn extract_account_data(
    changes: &mut SyncChanges,
    raw_event: Raw<AnyGlobalAccountDataEvent>,
) -> Result<(), TauriError> {
    let ev = raw_event.deserialize()?;

    match ev.event_type().to_string().as_str() {
        "org.opal-matrix.breadcrumbs" => return Ok(()),
        _ => (),
    }

    match ev {
        AnyGlobalAccountDataEvent::Direct(ev) => {
            let mut dms = HashSet::new();

            for (_, rooms) in ev.content.iter() {
                for room_id in rooms {
                    dms.insert(room_id.clone());
                }
            }

            changes.direct_rooms = Some(dms);
        }
        _ => match ev.event_type().to_string().as_str() {
            "org.opal-matrix.breadcrumbs" => return Ok(()),
            "im.vector.setting.breadcrumbs" => return Ok(()),
            _ => trace!("Unhandled global account data event: {:?}", ev),
        },
    }

    Ok(())
}

fn extract_state(
    changes: &mut SyncChanges,
    room_id: &OwnedRoomId,
    ev: AnySyncStateEvent,
    _state_event: Raw<AnySyncStateEvent>,
) -> Result<(), TauriError> {
    let Some(or) = ev.original_content() else {
        return Ok(());
    };

    let state_key = ev.state_key().to_string();

    let update = changes.room_updates.entry(room_id.clone()).or_default();

    match or {
        AnyStateEventContent::RoomName(ev) => {
            update.name = Some(ev.name.clone());
        }
        AnyStateEventContent::RoomAvatar(ev) => {
            let Some(url) = ev.url else {
                return Ok(());
            };

            update.avatar_url = Some(url.as_str().into());
        }
        AnyStateEventContent::RoomTopic(ev) => {
            update.topic = Some(ev.topic.clone());
        }
        AnyStateEventContent::RoomEncryption(ev) => {
            update.algorithm = Some(ev.algorithm.as_str().to_string());
        }
        AnyStateEventContent::RoomCreate(ev) => {
            if let Some(room_type) = ev.room_type {
                update.room_type = Some(room_type.as_str().to_string());
            }
        }
        AnyStateEventContent::RoomMember(ev) => {
            changes.member_updates.push(MemberRow {
                room_id: room_id.to_string(),
                user_id: state_key,
                display_name: ev.displayname.clone(),
                avatar_url: ev.avatar_url.clone().map(|u| u.as_str().to_string()),
                membership: ev.membership.into(),
            });
        }
        AnyStateEventContent::RoomPowerLevels(ev) => {
            update.power_levels = Some(serde_json::to_string(&ev).unwrap_or_default());
        }
        AnyStateEventContent::RoomGuestAccess(ev) => {
            update.guest_access = Some(ev.guest_access.to_string());
        }
        AnyStateEventContent::RoomHistoryVisibility(ev) => {
            update.history_visibility = Some(ev.history_visibility.to_string());
        }
        AnyStateEventContent::RoomJoinRules(ev) => {
            update.join_rule = Some(ev.join_rule.as_str().to_string());
        }
        AnyStateEventContent::SpaceChild(ev) => {
            let is_deleted = ev.via.is_empty();

            changes.space_children.push(SpaceChildRow {
                parent_room_id: room_id.to_string(),
                child_room_id: state_key,
                order_str: ev.order.map(|v| v.to_string()),
                is_deleted: is_deleted,
            });
        }
        AnyStateEventContent::SpaceParent(ev) => {
            let is_deleted = ev.via.is_empty();

            changes.space_parents.push(SpaceParentRow {
                child_room_id: room_id.to_string(),
                parent_room_id: state_key,
                is_canonical: ev.canonical,
                is_deleted: is_deleted,
            });
        }
        // Handled in special state
        AnyStateEventContent::CallMember(_) => (),
        _ => {
            trace!("Unhandled state event in room {}: {:?}", room_id, ev);
        }
    }

    Ok(())
}

fn extract_timeline(
    changes: &mut SyncChanges,
    room_id: &OwnedRoomId,
    ev: AnySyncTimelineEvent,
    raw_json: &RawValue,
    state_event: Raw<AnySyncTimelineEvent>,
) -> Result<(), TauriError> {
    match ev {
        AnySyncTimelineEvent::MessageLike(ev) => extract_message(changes, room_id, ev, raw_json)?,
        AnySyncTimelineEvent::State(ev) => {
            let raw_json_box = state_event.into_json();

            let raw_state_event: Raw<AnySyncStateEvent> = Raw::from_json(raw_json_box);
            extract_state(changes, room_id, ev.clone(), raw_state_event)?;

            if let Some(msg) = create_system_message(ev, room_id, raw_json.get().to_string()) {
                changes.new_messages.push(msg);
            }
        }
    }

    Ok(())
}

fn extract_message(
    changes: &mut SyncChanges,
    room_id: &OwnedRoomId,
    ev: AnySyncMessageLikeEvent,
    raw_json: &RawValue,
) -> Result<(), TauriError> {
    match ev {
        AnySyncMessageLikeEvent::Message(ev) => {
            if let Some(or) = ev.as_original() {
                warn!("Unimplemented message type in room {}: {:?}", room_id, or);
            }
        }
        AnySyncMessageLikeEvent::RoomMessage(ev) => {
            changes.new_messages.push(MessageRow {
                event_id: ev.event_id().to_string(),
                room_id: room_id.to_string(),
                sender: ev.sender().to_string(),
                raw_json: raw_json.get().to_string(),
                msg_type: "m.room.message".to_string(),
                timestamp: ev.origin_server_ts().as_secs().into(),
            });
        }
        AnySyncMessageLikeEvent::RoomRedaction(ev) => {
            changes.new_messages.push(MessageRow {
                event_id: ev.event_id().to_string(),
                room_id: room_id.to_string(),
                sender: ev.sender().to_string(),
                raw_json: raw_json.get().to_string(),
                msg_type: "m.room.redaction".to_string(),
                timestamp: ev.origin_server_ts().as_secs().into(),
            });
        }
        AnySyncMessageLikeEvent::RoomEncrypted(ev) => changes.new_messages.push(MessageRow {
            event_id: ev.event_id().to_string(),
            room_id: room_id.to_string(),
            sender: ev.sender().to_string(),
            raw_json: raw_json.get().to_string(),
            msg_type: "m.room.encrypted".to_string(),
            timestamp: ev.origin_server_ts().as_secs().into(),
        }),
        AnySyncMessageLikeEvent::Reaction(ev) => {
            changes.new_messages.push(MessageRow {
                event_id: ev.event_id().to_string(),
                room_id: room_id.to_string(),
                sender: ev.sender().to_string(),
                raw_json: raw_json.get().to_string(),
                msg_type: "m.reaction".to_string(),
                timestamp: ev.origin_server_ts().as_secs().into(),
            });
        }
        _ => return Ok(()),
    }

    Ok(())
}

fn create_system_message(
    state_ev: AnySyncStateEvent,
    room_id: &OwnedRoomId,
    raw_json: String,
) -> Option<MessageRow> {
    let event_id = state_ev.event_id().to_string();
    let sender = state_ev.sender().to_string();
    let msg_type = state_ev.event_type().to_string();
    let timestamp = state_ev.origin_server_ts().as_secs().into();

    return Some(MessageRow {
        event_id,
        room_id: room_id.to_string(),
        sender,
        msg_type,
        raw_json: raw_json,
        timestamp,
    });
}
