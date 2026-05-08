use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};

use log::{trace, warn};
use ruma::{
    OwnedRoomId, UInt,
    api::{
        IncomingResponse, OutgoingRequest,
        auth_scheme::SendAccessToken,
        client::{
            filter::FilterDefinition,
            sync::sync_events::v3::{
                Filter, Request as SyncRequest, Response as SyncResponse, State as SyncState,
            },
        },
    },
    events::{
        AnyGlobalAccountDataEvent, AnyRoomAccountDataEvent, AnyStateEventContent,
        AnySyncEphemeralRoomEvent, AnySyncMessageLikeEvent, AnySyncStateEvent,
        AnySyncTimelineEvent, presence::PresenceEventContent,
    },
    presence::PresenceState,
    serde::Raw,
};

use serde_json::value::RawValue;
use shared::{
    messages::UiMessage,
    user_profile::{PresenceInfo, PresenceStatus},
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_http::reqwest::{self, Client};

use crate::{
    TauriError, create_http_response,
    frontend::{send_member_update, send_sidebar_update},
    matrix_api::{crypto, handle_sync_calls, rooms::backfill_gap},
    state::{AppState, HomeServerInfo},
    storage::{
        members::MemberRow,
        messages::MessageRow,
        receipts::ReadReceiptRow,
        rooms::{SpaceChildRow, SpaceParentRow},
    },
};

use tokio_util::sync::CancellationToken;

async fn matrix_sync(
    server_info: &HomeServerInfo,
    access_token: &String,
    since: Option<String>,
) -> Result<SyncResponse, TauriError> {
    let mut req = SyncRequest::new();

    req.timeout = if since.is_some() {
        Some(std::time::Duration::from_secs(30))
    } else {
        Some(std::time::Duration::from_secs(0))
    };
    req.since = since.clone();
    req.set_presence = PresenceState::Online;

    let mut filter = FilterDefinition::empty();

    filter.room.timeline.limit = Some(UInt::try_from(100)?);

    req.filter = Some(Filter::FilterDefinition(filter));

    let req = req.try_into_http_request::<Vec<u8>>(
        &server_info.base_url,
        SendAccessToken::Always(access_token),
        Cow::Owned(server_info.supported_versions.clone()),
    )?;

    let http_req = reqwest::Request::try_from(req)?;

    let res = create_http_response(Client::new().execute(http_req).await?).await?;

    let sync_response = SyncResponse::try_from_http_response(res)?;

    Ok(sync_response)
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
        let (access_token, server_info) = state.get_api().await?;

        let sync_res = matrix_sync(&server_info, &access_token, since.clone()).await?;
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

        let res = crypto::process_sync_response(
            &olm_machine,
            sync_res,
            &access_token,
            &server_info.base_url,
        )
        .await?;

        handle_sync_response(&state, &handle, res).await?;

        state.save_session().await?;
    }

    Ok(())
}

use crate::storage::{SyncChanges, apply_sync_changes, handle_safe_stuff};

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

async fn handle_sync_response(
    state: &Arc<AppState>,
    handle: &AppHandle,
    response: SyncResponse,
) -> Result<(), TauriError> {
    let mut changes = SyncChanges::default();

    for raw_event in response.account_data.events {
        if let Err(e) = extract_account_data(&mut changes, raw_event) {
            log::error!("Error extracting account data: {:?}", e);
        }
    }

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

        for raw_event in room.account_data.events {
            let data = match raw_event.deserialize() {
                Ok(data) => data,
                Err(e) => {
                    warn!(
                        "Skipping malformed account data event in room {}: {} | raw={}",
                        room_id,
                        e,
                        raw_event.json().get()
                    );
                    continue;
                }
            };

            if let Err(e) = extract_room_account_data(&mut changes, &room_id, data) {
                warn!("Error extracting account data event: {:?}", e);
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
                || update.highlight_count.is_some()
                || update.notification_count.is_some()
        });

    {
        let mut conn_guard = state.connection.lock().await;
        let conn = conn_guard
            .as_mut()
            .ok_or("Database connection not initialized")?;

        let server_info = {
            let guard = state.home_server_info.read().await;
            guard.as_ref().cloned().ok_or("Matrix URL not set")?
        };
        let access_token = state.check_token().await?;

        let res = apply_sync_changes(conn, changes.clone()).await?;
        let stuff = handle_sync_calls(server_info, access_token, res).await?;
        handle_safe_stuff(conn, stuff).await?;

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

fn extract_room_account_data(
    changes: &mut SyncChanges,
    room_id: &OwnedRoomId,
    ev: AnyRoomAccountDataEvent,
) -> Result<(), TauriError> {
    match ev.event_type().to_string().as_str() {
        "org.opal-matrix.breadcrumbs" => return Ok(()),
        _ => (),
    }

    match ev {
        AnyRoomAccountDataEvent::FullyRead(ev) => {
            changes.read_receipts.push(ReadReceiptRow {
                room_id: room_id.to_string(),
                user_id: "".to_string(),
                receipt_type: "m.fully_read".to_string(),
                event_id: ev.content.event_id.to_string(),
                ts: 0,
            });
        }
        _ => (),
    }

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
