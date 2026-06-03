use matrix_sdk::{
    Client,
    ruma::{UserId, events::room::member::OriginalSyncRoomMemberEvent},
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
    let avatar_url = client
        .account()
        .get_avatar_url()
        .await?
        .map(|m| m.to_string());
    let update = UserProfile {
        display_name,
        avatar_url,
        user_id,
    };
    send_user_profile_update(handle, update)
}

pub fn client_user_profile_event_handle(
    handle: &AppHandle,
    own_id: &str,
    event: OriginalSyncRoomMemberEvent,
) {
    if event.state_key.as_str() != own_id {
        return;
    }
    let profile = UserProfile {
        display_name: event.content.displayname.clone(),
        avatar_url: event.content.avatar_url.as_ref().map(|m| m.to_string()),
        user_id: own_id.to_string(),
    };
    if let Err(e) = send_user_profile_update(handle, profile) {
        log::error!("Failed to send user profile update: {e:?}");
    };
}
