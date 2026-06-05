use std::collections::HashMap;

use matrix_sdk::ruma::{events::presence::PresenceEvent, presence::PresenceState, serde::Raw};
use shared::profile::{PresenceInfo, PresenceStatus};
use tauri::{AppHandle, Emitter};

fn presence_to_ui(state: PresenceState) -> PresenceStatus {
    match state {
        PresenceState::Online => PresenceStatus::Online,
        PresenceState::Offline => PresenceStatus::Offline,
        PresenceState::Unavailable => PresenceStatus::Unavailable,
        _ => PresenceStatus::Offline,
    }
}

pub fn handle_presences(presence_events: &Vec<Raw<PresenceEvent>>, app_handle: &AppHandle) {
    let mut presence_batch = HashMap::new();

    for raw_event in presence_events {
        if let Ok(event) = raw_event.deserialize() {
            let user_id = event.sender.to_string();

            let info = PresenceInfo {
                status_msg: event.content.status_msg.clone(),
                status: presence_to_ui(event.content.presence),
                last_active_ago: event.content.last_active_ago.map(|v| v.into()),
            };

            presence_batch.insert(user_id, info);
        } else {
            log::warn!("Failed to deserialize a presence event");
        }
    }

    if !presence_batch.is_empty() {
        send_presence_update(app_handle.clone(), &presence_batch).unwrap_or_else(|e| {
            log::error!("Failed to send presence update: {:?}", e);
        });
    }
}

pub fn send_presence_update(
    handle: AppHandle,
    payload: &HashMap<String, PresenceInfo>,
) -> Result<(), tauri::Error> {
    log::debug!("Sending presence update for {} rooms", payload.len());
    handle.emit("presence_update", payload)
}
