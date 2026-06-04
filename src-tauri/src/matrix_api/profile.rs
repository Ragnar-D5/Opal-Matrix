use std::sync::{Arc, Mutex};

use matrix_sdk::{
    event_handler::Ctx, ruma::events::room::member::OriginalSyncRoomMemberEvent, Client,
};
use shared::user_profile::UserProfile;
use tauri::{AppHandle, Emitter};

use crate::TauriError;

pub fn send_user_profile_update(handle: &AppHandle, update: UserProfile) -> Result<(), TauriError> {
    handle
        .emit("user_profile", update)
        .map_err(|e| format!("Failed to send user profile update: {e}").into())
}

pub async fn send_user_to_frontend(handle: &AppHandle, client: &Client) -> Result<(), TauriError> {
    let user_id = client.user_id().ok_or("Not logged in")?.to_string();
    let display_name = client.account().get_display_name().await?;
    let update = UserProfile {
        display_name,
        user_id,
    };
    send_user_profile_update(handle, update)
}

pub async fn client_user_profile_event_handle(
    event: OriginalSyncRoomMemberEvent,
    handle: Ctx<AppHandle>,
    client: Client,
    debounce: Ctx<Arc<Mutex<ProfileDebounce>>>,
) {
    let Some(own_id) = client.user_id().map(|i| i.to_string()) else {
        log::error!("Received profile event but client has no user ID");
        return;
    };

    if event.state_key.as_str() != own_id {
        return;
    }

    if let Some(prev) = event.prev_content() {
        if prev.membership != event.content.membership {
            return;
        }
        if prev.displayname == event.content.displayname
            && prev.avatar_url == event.content.avatar_url
        {
            return;
        }
    }

    {
        let mut state = debounce.lock().unwrap();
        if state.timer_running {
            state.pending = true;
            return;
        }
        state.timer_running = true;
    }

    if let Err(e) = send_user_to_frontend(&handle, &client).await {
        log::error!("Failed to send user profile update: {e:?}");
    }

    let debounce_clone = debounce.clone();
    let handle_clone = handle.clone();
    let client_clone = client.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let should_emit = {
            let mut state = debounce_clone.lock().unwrap();
            let pending = state.pending;
            state.pending = false;
            state.timer_running = false;
            pending
        };

        if should_emit
            && let Err(e) = send_user_to_frontend(&handle_clone, &client_clone).await {
                log::error!("Failed to send user profile update after debounce: {e:?}");
            }
    });
}

#[derive(Debug, Default)]
pub struct ProfileDebounce {
    pending: bool,
    timer_running: bool,
}
