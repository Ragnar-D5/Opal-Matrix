use std::collections::HashMap;

use matrix_sdk::{
    event_handler::Ctx,
    ruma::{events::room::member::OriginalSyncRoomMemberEvent, profile::ProfileFieldName, UserId},
    Room, RoomMemberships,
};
use shared::profile::{MemberProfile, UserProfile};
use tauri::{command, AppHandle, Emitter};

use crate::{MatrixClientState, TauriError};

pub async fn on_member_update(
    event: OriginalSyncRoomMemberEvent,
    room: Room,
    app_handle: Ctx<AppHandle>,
) {
    let content = event.content;

    let profile = MemberProfile {
        room_id: room.room_id().to_string(),
        profile: UserProfile {
            user_id: event.state_key.to_string(),
            display_name: content.displayname,
            has_avatar: content.avatar_url.is_some(),
        },
    };

    let payload = HashMap::from([(room.room_id().to_string(), vec![profile.clone()])]);
    send_member_update(&app_handle, payload).unwrap_or_else(|e| {
        log::error!("Failed to send member update: {:?}", e);
    });
}

pub async fn send_all_members(handle: &AppHandle, rooms: &[Room]) -> Result<(), TauriError> {
    let mut payload = HashMap::new();

    for room in rooms {
        let room_id = room.room_id().to_string();
        let members = room.members(RoomMemberships::JOIN).await?;

        let profiles = members
            .into_iter()
            .map(|member| MemberProfile {
                room_id: room_id.clone(),
                profile: UserProfile {
                    user_id: member.user_id().to_string(),
                    display_name: member.display_name().map(|s| s.to_string()),
                    has_avatar: member.avatar_url().is_some(),
                },
            })
            .collect();

        payload.insert(room_id, profiles);
    }

    send_member_update(handle, payload)?;

    Ok(())
}

pub fn send_member_update(
    handle: &AppHandle,
    payload: HashMap<String, Vec<MemberProfile>>,
) -> Result<(), TauriError> {
    log::debug!("Updating {} members", payload.len());

    handle.emit("member_update", payload)?;

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn get_user_profile(
    user_id: String,
    client: MatrixClientState<'_>,
) -> Result<UserProfile, TauriError> {
    let client = client.read().await;

    let user_id = UserId::parse(user_id)?;

    let display_name = client
        .account()
        .fetch_profile_field_of(user_id.clone(), ProfileFieldName::DisplayName)
        .await?
        .map(|v| v.value().to_string());

    let has_avatar = client
        .account()
        .fetch_profile_field_of(user_id.clone(), ProfileFieldName::AvatarUrl)
        .await?
        .is_some();

    Ok(UserProfile {
        user_id: user_id.to_string(),
        display_name,
        has_avatar,
    })
}
