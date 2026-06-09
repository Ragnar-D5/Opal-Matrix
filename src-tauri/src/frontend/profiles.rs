use std::collections::HashMap;

use futures::future::join_all;
use matrix_sdk::{
    Client, Room, RoomMemberships,
    event_handler::Ctx,
    ruma::{
        OwnedUserId, UserId,
        events::{room::member::OriginalSyncRoomMemberEvent, typing::SyncTypingEvent},
        profile::ProfileFieldName,
    },
};
use shared::profile::{CustomProperties, MemberProfile, UserProfile};
use tauri::{AppHandle, Emitter, command};

use crate::{MatrixClientState, TauriError, matrix_api::profile::get_custom_fields};

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

            custom_properties: get_custom_fields(&room.client(), event.state_key.clone()).await,
        },
    };

    let payload = HashMap::from([(room.room_id().to_string(), vec![profile.clone()])]);
    send_member_update(&app_handle, payload).unwrap_or_else(|e| {
        log::error!("Failed to send member update: {:?}", e);
    });
}

pub async fn send_all_members(
    client: &Client,
    handle: &AppHandle,
    rooms: &[Room],
) -> Result<(), TauriError> {
    let mut payload: HashMap<String, Vec<MemberProfile>> = HashMap::new();
    // user_id -> list of (room_id, has_avatar, display_name) across all rooms
    let mut user_memberships: HashMap<OwnedUserId, Vec<(String, bool, Option<String>)>> =
        HashMap::new();

    for room in rooms {
        let room_id = room.room_id().to_string();
        let members = room.members(RoomMemberships::JOIN).await?;

        let profiles: Vec<MemberProfile> = members
            .into_iter()
            .map(|member| {
                let user_id = member.user_id().to_owned();
                let has_avatar = member.avatar_url().is_some();
                let display_name = member.display_name().map(|s| s.to_string());

                user_memberships
                    .entry(user_id.clone())
                    .or_default()
                    .push((room_id.clone(), has_avatar, display_name.clone()));

                MemberProfile {
                    room_id: room_id.clone(),
                    profile: UserProfile {
                        user_id: user_id.to_string(),
                        display_name,
                        has_avatar,
                        custom_properties: CustomProperties::from_user_id(user_id.as_str()),
                    },
                }
            })
            .collect();

        payload.insert(room_id, profiles);
    }

    // Emit immediately with derived properties so the UI renders right away
    send_member_update(handle, payload)?;

    // Fetch custom fields for each unique user in parallel (one fetch per user, not per membership)
    let futs: Vec<_> = user_memberships
        .keys()
        .cloned()
        .map(|user_id| {
            let client = client.clone();
            async move {
                let props = get_custom_fields(&client, user_id.clone()).await;
                (user_id, props)
            }
        })
        .collect();

    let results = join_all(futs).await;

    let mut update_payload: HashMap<String, Vec<MemberProfile>> = HashMap::new();
    for (user_id, custom_properties) in results {
        for (room_id, has_avatar, display_name) in &user_memberships[&user_id] {
            update_payload
                .entry(room_id.clone())
                .or_default()
                .push(MemberProfile {
                    room_id: room_id.clone(),
                    profile: UserProfile {
                        user_id: user_id.to_string(),
                        display_name: display_name.clone(),
                        has_avatar: *has_avatar,
                        custom_properties: custom_properties.clone(),
                    },
                });
        }
    }

    send_member_update(handle, update_payload)?;

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

        custom_properties: get_custom_fields(&client, user_id.clone()).await,
    })
}

pub async fn handle_typing_notice(event: SyncTypingEvent, room: Room, handle: Ctx<AppHandle>) {
    let room_id = room.room_id().to_string();
    let user_ids: Vec<String> = event
        .content
        .user_ids
        .iter()
        .map(|v| v.to_string())
        .collect();

    if let Err(e) = handle.emit("typing_update", (room_id, user_ids)) {
        log::error!("Failed to send typing update: {:?}", e);
    }
}
