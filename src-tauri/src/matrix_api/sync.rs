use std::collections::HashSet;
use std::fmt::format;

use log::{debug, trace, warn};
use ruma::serde::Raw;
use serde_json::value::RawValue;
use tauri::{AppHandle, Emitter};

use crate::frontend::{build_tree, send_sidebar_update};
use crate::storage::members::MemberRow;
use crate::storage::messages::MessageRow;
use crate::storage::rooms::{SpaceChildRow, SpaceParentRow};
use crate::{construct_url, matrix_api::crypto, AppState, TauriError};
use reqwest::Client;

use ruma::api::{client::sync::sync_events::v3::Response as SyncResponse, IncomingResponse};
use ruma::OwnedRoomId;
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

    if let Some(since_token) = since {
        let params = [("since", since_token), ("timeout", 30000.to_string())];

        url = reqwest::Url::parse_with_params(url.as_str(), params)?;
    }

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

use ruma::api::client::sync::sync_events::v3::State as SyncState;
use ruma::events::{
    AnyGlobalAccountDataEvent, AnyStateEventContent, AnySyncMessageLikeEvent, AnySyncStateEvent,
    AnySyncTimelineEvent,
};
async fn handle_sync_response(
    state: &std::sync::Arc<AppState>,
    handle: &AppHandle,
    response: SyncResponse,
) -> Result<(), TauriError> {
    let mut changes = SyncChanges::default();

    for raw_event in response.account_data.events {
        extract_account_data(&mut changes, raw_event)?;
    }

    for (room_id, room) in response.rooms.join {
        changes.joined_rooms.push(room_id.clone());

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
            extract_state(&mut changes, &room_id, data, state_event.clone())?;
        }

        for raw_event in room.timeline.events {
            let clone = raw_event.clone();
            let raw_json = clone.json();

            let data = raw_event.deserialize()?;
            extract_timeline(&mut changes, &room_id, data, raw_json, raw_event)?;
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

        if sidebar_needs_update {
            send_sidebar_update(conn, handle)?;
        }
    }

    Ok(())
}

fn extract_account_data(
    changes: &mut SyncChanges,
    raw_event: Raw<AnyGlobalAccountDataEvent>,
) -> Result<(), TauriError> {
    let ev = raw_event.deserialize()?;

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
        _ => {
            trace!("Unhandled global account data event: {:?}", ev);
        }
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

                // changes.new_messages.push(MessageRow {
                //     event_id: or.event_id.to_string(),
                //     room_id: room_id.to_string(),
                //     sender: or.sender.to_string(),
                //     body: None,
                //     raw_json: raw_json.get().to_string(),
                //     msg_type: "message".to_string(),
                //     timestamp: or.origin_server_ts.as_secs().into(),
                // });
            }
        }
        AnySyncMessageLikeEvent::RoomMessage(ev) => {
            if let Some(or) = ev.as_original() {
                let (msg_type, body) = match or.content.msgtype.clone() {
                    ruma::events::room::message::MessageType::Text(text_content) => {
                        ("m.text", Some(text_content.body.clone()))
                    }
                    ruma::events::room::message::MessageType::Image(image_content) => {
                        ("m.image", Some(image_content.body.clone()))
                    }
                    ruma::events::room::message::MessageType::Audio(audio_content) => {
                        ("m.audio", Some(audio_content.body.clone()))
                    }
                    ruma::events::room::message::MessageType::Video(video_content) => {
                        ("m.video", Some(video_content.body.clone()))
                    }
                    ruma::events::room::message::MessageType::File(file_content) => {
                        ("m.file", Some(file_content.body.clone()))
                    }
                    _ => ("m.unknown", None),
                };

                changes.new_messages.push(MessageRow {
                    event_id: or.event_id.to_string(),
                    room_id: room_id.to_string(),
                    sender: or.sender.to_string(),
                    body: body,
                    raw_json: raw_json.get().to_string(),
                    msg_type: msg_type.to_string(),
                    timestamp: or.origin_server_ts.as_secs().into(),
                });
            }
        }
        _ => return Ok(()),
    }

    Ok(())
}

use ruma::events::room::member::MembershipState;
fn create_system_message(
    state_ev: AnySyncStateEvent,
    room_id: &OwnedRoomId,
    raw_json: String,
) -> Option<MessageRow> {
    let Some(or) = state_ev.original_content() else {
        return None;
    };

    let event_id = state_ev.event_id().to_string();
    let sender = state_ev.sender().to_string();
    let msg_type = state_ev.event_type().to_string();
    let timestamp = state_ev.origin_server_ts().as_secs().into();

    let body = match or {
        AnyStateEventContent::RoomCreate(_) => {
            format!("{} created the room", sender)
        }
        AnyStateEventContent::RoomName(ev) => {
            format!("{} changed the room name to {}", sender, ev.name)
        }
        AnyStateEventContent::RoomMember(ev) => {
            let target = state_ev.state_key().to_string();

            match ev.membership {
                MembershipState::Join => {
                    if sender == target {
                        format!("{} joined the room", sender)
                    } else {
                        format!("{} joined", target)
                    }
                }
                MembershipState::Invite => format!("{} invited {}", sender, target),
                MembershipState::Leave => {
                    if sender == target {
                        format!("{} left the room", sender)
                    } else {
                        format!("{} kicked {}", sender, target)
                    }
                }
                MembershipState::Ban => format!("{} banned {}", sender, target),
                _ => return None,
            }
        }
        AnyStateEventContent::RoomEncryption(_) => format!("{} enabled room encryption", sender),
        _ => return None,
    };

    return Some(MessageRow {
        event_id,
        room_id: room_id.to_string(),
        sender,
        msg_type,
        body: Some(body),
        raw_json: raw_json,
        timestamp,
    });
}
