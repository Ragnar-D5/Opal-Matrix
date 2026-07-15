use std::collections::HashMap;

use futures::future::join_all;
use futures_util::StreamExt;
use matrix_sdk::{
    Client, Room, RoomMemberships,
    event_handler::Ctx,
    ruma::{
        OwnedRoomId, OwnedUserId,
        events::{room::member::OriginalSyncRoomMemberEvent, typing::SyncTypingEvent},
        profile::ProfileFieldName,
    },
};
use shared::{
    api::events::TypingUpdate,
    profile::{CustomProperties, MemberProfile, UserProfile},
};
use tauri::{AppHandle, State, command};
use tokio::sync::RwLock;

use crate::{TauriError, matrix_api::profile::get_custom_fields, send_event};

pub async fn on_member_update(
    event: OriginalSyncRoomMemberEvent,
    room: Room,
    app_handle: Ctx<AppHandle>,
) {
    let content = event.content;

    let custom_properties = get_custom_fields(&room.client(), event.state_key.clone()).await;

    let profile = MemberProfile {
        room_id: room.room_id().to_owned(),
        profile: UserProfile {
            user_id: event.state_key,
            display_name: content.displayname,
            has_avatar: content.avatar_url.is_some(),

            custom_properties,
        },
    };

    let payload = HashMap::from([(room.room_id().to_string(), vec![profile])]);
    send_event(&app_handle, &payload);
}

type RoomMembershipsType = Vec<(OwnedRoomId, Vec<(OwnedUserId, bool, Option<String>)>)>;

pub async fn send_all_members(
    client: &Client,
    handle: &AppHandle,
    rooms: &[Room],
) -> Result<(), TauriError> {
    let room_memberships: RoomMembershipsType = futures_util::stream::iter(rooms.iter().cloned())
        .map(|room| async move {
            let room_id = room.room_id().to_owned();
            let members = match room.members(RoomMemberships::all()).await {
                Ok(members) => members,
                Err(e) => {
                    log::error!("Failed to get members for room {}: {:?}", room_id, e);
                    return None;
                }
            };

            let mut memberships = Vec::with_capacity(members.len());
            let profiles: Vec<MemberProfile> = members
                .into_iter()
                .map(|member| {
                    let user_id = member.user_id().to_owned();
                    let has_avatar = member.avatar_url().is_some();
                    let display_name = member.display_name().map(|s| s.to_string());

                    memberships.push((user_id.clone(), has_avatar, display_name.clone()));

                    MemberProfile {
                        room_id: room_id.clone(),
                        profile: UserProfile {
                            custom_properties: CustomProperties::from_user_id(&user_id),
                            user_id,
                            display_name,
                            has_avatar,
                        },
                    }
                })
                .collect();

            Some((room_id, profiles, memberships))
        })
        .buffer_unordered(16)
        .filter_map(|entry| async move { entry })
        .map(|(room_id, profiles, memberships)| {
            send_event(handle, &HashMap::from([(room_id.clone(), profiles)]));
            (room_id, memberships)
        })
        .collect()
        .await;

    let mut user_memberships: HashMap<OwnedUserId, Vec<(OwnedRoomId, bool, Option<String>)>> =
        HashMap::new();
    for (room_id, memberships) in room_memberships {
        for (user_id, has_avatar, display_name) in memberships {
            user_memberships.entry(user_id).or_default().push((
                room_id.clone(),
                has_avatar,
                display_name,
            ));
        }
    }

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

    let mut update_payload: HashMap<OwnedRoomId, Vec<MemberProfile>> = HashMap::new();
    for (user_id, custom_properties) in results {
        for (room_id, has_avatar, display_name) in &user_memberships[&user_id] {
            update_payload
                .entry(room_id.clone())
                .or_default()
                .push(MemberProfile {
                    room_id: room_id.clone(),
                    profile: UserProfile {
                        user_id: user_id.clone(),
                        display_name: display_name.clone(),
                        has_avatar: *has_avatar,
                        custom_properties: custom_properties.clone(),
                    },
                });
        }
    }

    send_event(handle, &update_payload);

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn get_user_profile(
    user_id: OwnedUserId,
    client: State<'_, RwLock<Client>>,
) -> Result<UserProfile, TauriError> {
    let client = client.read().await;

    let account = client.account();

    let (display_name_result, avatar_result, custom_properties) = tokio::join!(
        account.fetch_profile_field_of(user_id.clone(), ProfileFieldName::DisplayName),
        account.fetch_profile_field_of(user_id.clone(), ProfileFieldName::AvatarUrl),
        get_custom_fields(&client, user_id.clone()),
    );

    let display_name = display_name_result
        .ok()
        .flatten()
        .and_then(|v| v.value().as_str().map(|t| t.to_string()));
    let has_avatar = avatar_result.ok().flatten().is_some();

    let profile = UserProfile {
        user_id,
        display_name,
        has_avatar,
        custom_properties,
    };

    Ok(profile)
}

pub async fn handle_typing_notice(event: SyncTypingEvent, room: Room, handle: Ctx<AppHandle>) {
    send_event(
        &handle,
        &TypingUpdate {
            room_id: room.room_id().to_owned(),
            user_ids: event.content.user_ids,
        },
    );
}
