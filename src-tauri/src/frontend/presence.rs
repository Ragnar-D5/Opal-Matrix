use matrix_sdk::ruma::{events::presence::PresenceEvent, serde::Raw};
use shared::api::events::PresenceUpdate;
use tauri::AppHandle;

use crate::send_event;

pub fn handle_presences(presence_events: &[Raw<PresenceEvent>], app_handle: &AppHandle) {
    let presence_batch: PresenceUpdate = presence_events
        .iter()
        .filter_map(|raw| {
            raw.deserialize()
                .map_err(|e| log::error!("Failed to deserialize a presence event: {e}"))
                .map(|event| (event.sender, event.content.into()))
                .ok()
        })
        .collect();

    if !presence_batch.is_empty() {
        send_event(app_handle, &presence_batch);
    }
}
