use std::collections::HashMap;

use matrix_sdk::{
    event_handler::Ctx,
    room::RoomMember,
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
pub async fn get_member_for_room(
    client: MatrixClientState<'_>,
    room_id: String,
    user_id: String,
) -> Result<MemberProfile, TauriError> {
    log::debug!("Getting member for room: {}", &room_id);
    let room = client
        .read()
        .await
        .get_room(&RoomId::parse(&room_id)?)
        .ok_or(format!("Room not found: {}", &room_id))?;

    let member: RoomMember = room
        .get_member(&UserId::parse(&user_id)?)
        .await?
        .ok_or(format!(
            "Membership for user {user_id} in room {room_id} not found"
        ))?;

    Ok(MemberProfile {
        room_id,
        profile: UserProfile {
            user_id,
            display_name: member.display_name().map(|s| s.to_string()),
            has_avatar: member.avatar_url().is_some(),
        },
    })
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
    log::debug!("Sending {} member updates", payload.len());

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
