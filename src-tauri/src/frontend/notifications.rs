use std::sync::Arc;

use matrix_sdk::{
    Room,
    event_handler::Ctx,
    media::{MediaFormat, MediaThumbnailSettings},
    room::RoomMember,
    ruma::{
        events::room::message::{MessageType, OriginalSyncRoomMessageEvent},
        push::{Action, Tweak},
    },
};
use tauri::{AppHandle, Manager};

use crate::{send_notification, state::AppState};

/// Converts a [`core::time::Duration`] to a human-readable string, using seconds, minutes, and hours.
fn duration_to_string(duration: core::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs == 0 {
        "now".to_string()
    } else if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        format!("{}h", secs / 3600)
    }
}

/// Fetches the sender's avatar as a small thumbnail and writes it to the app's cache
/// directory, returning the path to use as a notification icon. The file is keyed by
/// the avatar's media ID, so it's only downloaded and written once per distinct avatar.
async fn cached_avatar_icon_path(handle: &AppHandle, member: &RoomMember) -> Option<String> {
    let media_id = member.avatar_url()?.media_id().ok()?.to_string();

    let cache_dir = handle.path().app_cache_dir().ok()?.join("avatar_icons");
    let path = cache_dir.join(&media_id);

    if !path.exists() {
        let settings = MediaThumbnailSettings::new(128u32.into(), 128u32.into());
        let bytes = match member.avatar(MediaFormat::Thumbnail(settings)).await {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return None,
            Err(e) => {
                log::warn!("Failed to fetch avatar for notification icon: {}", e);
                return None;
            }
        };

        if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
            log::warn!("Failed to create avatar icon cache dir: {}", e);
            return None;
        }
        if let Err(e) = tokio::fs::write(&path, bytes).await {
            log::warn!("Failed to write avatar icon to cache: {}", e);
            return None;
        }
    }

    Some(path.to_string_lossy().into_owned())
}

pub async fn on_message(
    event: OriginalSyncRoomMessageEvent,
    room: Room,
    actions: Vec<Action>,
    state: Ctx<Arc<AppState>>,
    handle: Ctx<AppHandle>,
) {
    if !*state.initial_sync_done.read().await {
        return;
    }

    if !actions.iter().any(|a| a.should_notify()) {
        return;
    }
    let is_highlight = actions
        .iter()
        .any(|a| matches!(a, Action::SetTweak(Tweak::Highlight(_))));

    let focused = *state.frontend_is_focused.read().await;
    let current = state.frontend_current_room_id.read().await.clone();
    if focused && current.as_deref() == Some(room.room_id().as_str()) {
        return;
    }

    log::debug!(
        "Notification: room={} is_highlight={}",
        room.room_id(),
        is_highlight
    );

    let sender = event.sender;
    let member = match room.get_member(&sender).await {
        Ok(member) => member,
        Err(e) => {
            log::warn!("Failed to get member: {}", e);
            return;
        }
    };

    let name = member
        .as_ref()
        .map(|m| m.name().to_string())
        .unwrap_or(sender.to_string());

    let icon = match &member {
        Some(member) => cached_avatar_icon_path(&handle, member).await,
        None => None,
    };

    let text = match event.content.msgtype {
        MessageType::Audio(audio) => {
            let duration = audio.info.and_then(|i| i.duration);

            format!(
                "{name} sent an audio message{}",
                duration.map(duration_to_string).unwrap_or_default()
            )
        }
        MessageType::Emote(emote) => emote.body,
        MessageType::File(file) => {
            let filename = file.filename();

            format!("Sent a file: {filename}")
        }
        MessageType::Image(image) => {
            let filename = image.filename();

            format!("Sent an image: {filename}")
        }
        MessageType::Location(_loc) => "Sent a location".to_string(),
        MessageType::Notice(notice) => {
            format!("Sent a notice: {}", notice.body)
        }
        MessageType::ServerNotice(notice) => {
            format!("Sent a server notice: {}", notice.body)
        }
        MessageType::Text(text) => text.body,
        MessageType::Video(video) => {
            let filename = video.filename();

            format!("Sent a video: {filename}")
        }
        _ => {
            return;
        }
    };

    let title = if room.compute_is_dm().await.unwrap_or(false) {
        name
    } else {
        match room.display_name().await {
            Ok(room_name) => format!("{name} in {room_name}"),
            Err(e) => {
                log::error!("Failed to get room name: {e}");
                name
            }
        }
    };

    if let Err(e) = send_notification(&handle, title, text, icon).await {
        log::warn!("Failed to send notification: {:?}", e);
    }
}
