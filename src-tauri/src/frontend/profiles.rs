use matrix_sdk::{
    event_handler::Ctx,
    ruma::{
        events::room::member::OriginalSyncRoomMemberEvent, profile::ProfileFieldName, RoomId,
        UserId,
    },
    Room, RoomMemberships,
};
use shared::profile::{MemberProfile, UserProfile};
use tauri::{command, AppHandle, Emitter};

use crate::{MatrixClientState, TauriError};

#[command(rename_all = "snake_case")]
pub async fn get_members_for_room(
    client: MatrixClientState<'_>,
    room_id: String,
) -> Result<Vec<MemberProfile>, TauriError> {
    log::debug!("Getting members for room: {}", &room_id);
    let room = client
        .read()
        .await
        .get_room(&RoomId::parse(&room_id)?)
        .ok_or(format!("Room not found: {}", &room_id))?;

    let sdk_members = room.members(RoomMemberships::ACTIVE).await?;

    let members: Vec<MemberProfile> = sdk_members
        .into_iter()
        .map(|m| MemberProfile {
            room_id: room_id.clone(),
            profile: UserProfile {
                user_id: m.user_id().to_string(),
                display_name: m.display_name().map(|v| v.to_string()),

                has_avatar: m.avatar_url().is_some(),
            },
        })
        .collect();

    log::debug!("Found {} members for room {}", members.len(), &room_id);

    Ok(members)
}

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

    send_member_update(&app_handle, profile).unwrap_or_else(|e| {
        log::error!("Failed to send member update: {:?}", e);
    });
}

pub fn send_member_update(handle: &AppHandle, payload: MemberProfile) -> Result<(), TauriError> {
    log::debug!("Sending member update for {}", payload.room_id);

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
