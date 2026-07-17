use std::{collections::HashMap, sync::Arc};

use futures::future::join_all;
use futures_util::StreamExt;
use matrix_sdk::{
    Client, Room, RoomMemberships,
    event_handler::Ctx,
    ruma::{
        OwnedMxcUri, OwnedRoomId, OwnedUserId,
        api::client::profile::{AvatarUrl, DisplayName},
        events::{room::member::OriginalSyncRoomMemberEvent, typing::SyncTypingEvent},
    },
};
use shared::{
    api::events::TypingUpdate,
    profile::{CustomProperties, MemberProfile, UserProfile},
};
use tauri::{AppHandle, State, command};
use tokio::sync::RwLock;

use crate::{
    TauriError,
    matrix_api::profile::get_custom_fields,
    send_event,
    state::{AppState, CachedMemberProfile},
};

pub async fn on_member_update(
    event: OriginalSyncRoomMemberEvent,
    room: Room,
    app_handle: Ctx<AppHandle>,
    state: Ctx<Arc<AppState>>,
) {
    let user_id = event.state_key;
    let content = event.content;
    let room_id = room.room_id().to_owned();

    let cached = state
        .member_profile_cache
        .lock()
        .unwrap()
        .get(&(room_id.clone(), user_id.clone()))
        .cloned();

    let custom_properties = get_custom_fields(
        &room.client(),
        user_id.clone(),
        cached.as_ref().map(|c| c.custom_properties.clone()),
    )
    .await;

    let (display_name, avatar_url) = match room
        .client()
        .account()
        .fetch_user_profile_of(&user_id)
        .await
    {
        Ok(profile) => (
            profile.get_static::<DisplayName>().ok().flatten(),
            profile
                .get_static::<AvatarUrl>()
                .ok()
                .flatten()
                .or_else(|| cached.as_ref().and_then(|c| c.avatar_url.clone())),
        ),
        Err(e) => {
            log::debug!("Failed to fetch profile for {user_id}, using event content: {e:?}");
            (
                content.displayname,
                content
                    .avatar_url
                    .or_else(|| cached.as_ref().and_then(|c| c.avatar_url.clone())),
            )
        }
    };

    state.member_profile_cache.lock().unwrap().insert(
        (room_id.clone(), user_id.clone()),
        CachedMemberProfile {
            avatar_url: avatar_url.clone(),
            custom_properties: custom_properties.clone(),
        },
    );

    let profile = MemberProfile {
        room_id,
        profile: UserProfile {
            user_id,
            display_name,
            avatar_url,

            custom_properties,
        },
    };

    let payload = HashMap::from([(room.room_id().to_string(), vec![profile])]);
    send_event(&app_handle, &payload);
}

type RoomMembershipsType = Vec<(
    OwnedRoomId,
    Vec<(OwnedUserId, Option<OwnedMxcUri>, Option<String>)>,
)>;

type MapType = HashMap<OwnedUserId, Vec<(OwnedRoomId, Option<OwnedMxcUri>, Option<String>)>>;

pub async fn send_all_members(
    client: &Client,
    handle: &AppHandle,
    rooms: &[Room],
    state: &Arc<AppState>,
) -> Result<(), TauriError> {
    let room_memberships: RoomMembershipsType = futures_util::stream::iter(rooms.iter().cloned())
        .map(|room| {
            let state = state.clone();
            async move {
                let room_id = room.room_id().to_owned();
                let members = match room.members(RoomMemberships::all()).await {
                    Ok(members) => members,
                    Err(e) => {
                        log::error!("Failed to get members for room {}: {:?}", room_id, e);
                        return None;
                    }
                };

                let mut memberships = Vec::with_capacity(members.len());

                let mut profiles = Vec::new();

                for member in members {
                    let user_id = member.user_id().to_owned();
                    let display_name = member.display_name().map(|s| s.to_string());

                    let cached = state
                        .member_profile_cache
                        .lock()
                        .unwrap()
                        .get(&(room_id.clone(), user_id.clone()))
                        .cloned();

                    let avatar_url = if let Some(url) = member.avatar_url() {
                        Some(url.to_owned())
                    } else {
                        let fetched = match client.account().fetch_user_profile_of(&user_id).await
                        {
                            Ok(profile) => profile.get_static::<AvatarUrl>().ok().flatten(),
                            Err(e) => {
                                log::debug!(
                                    "Failed to fetch profile for {user_id}, using event content: {e:?}"
                                );
                                None
                            }
                        };
                        fetched.or_else(|| cached.as_ref().and_then(|c| c.avatar_url.clone()))
                    };

                    // Real custom properties are fetched in the second wave below;
                    // reuse a cached value here (if any) so this first, faster wave
                    // doesn't flash the user-id-derived default color before that
                    // wave corrects it.
                    let custom_properties = cached
                        .map(|c| c.custom_properties)
                        .unwrap_or_else(|| CustomProperties::from_user_id(&user_id));

                    memberships.push((user_id.clone(), avatar_url.clone(), display_name.clone()));

                    profiles.push(MemberProfile {
                        room_id: room_id.clone(),
                        profile: UserProfile {
                            custom_properties,
                            user_id,
                            display_name,
                            avatar_url,
                        },
                    });
                }

                Some((room_id, profiles, memberships))
            }
        })
        .buffer_unordered(16)
        .filter_map(|entry| async move { entry })
        .map(|(room_id, profiles, memberships)| {
            send_event(handle, &HashMap::from([(room_id.clone(), profiles)]));
            (room_id, memberships)
        })
        .collect()
        .await;

    let mut user_memberships: MapType = HashMap::new();
    for (room_id, memberships) in room_memberships {
        for (user_id, avatar_url, display_name) in memberships {
            user_memberships.entry(user_id).or_default().push((
                room_id.clone(),
                avatar_url,
                display_name,
            ));
        }
    }

    let futs: Vec<_> = user_memberships
        .keys()
        .cloned()
        .map(|user_id| {
            let client = client.clone();
            let fallback = user_memberships[&user_id].iter().find_map(|(room_id, ..)| {
                state
                    .member_profile_cache
                    .lock()
                    .unwrap()
                    .get(&(room_id.clone(), user_id.clone()))
                    .map(|c| c.custom_properties.clone())
            });
            async move {
                let props = get_custom_fields(&client, user_id.clone(), fallback).await;
                (user_id, props)
            }
        })
        .collect();

    let results = join_all(futs).await;

    let mut update_payload: HashMap<OwnedRoomId, Vec<MemberProfile>> = HashMap::new();
    for (user_id, custom_properties) in results {
        for (room_id, avatar_url, display_name) in &user_memberships[&user_id] {
            state.member_profile_cache.lock().unwrap().insert(
                (room_id.clone(), user_id.clone()),
                CachedMemberProfile {
                    avatar_url: avatar_url.clone(),
                    custom_properties: custom_properties.clone(),
                },
            );

            update_payload
                .entry(room_id.clone())
                .or_default()
                .push(MemberProfile {
                    room_id: room_id.clone(),
                    profile: UserProfile {
                        user_id: user_id.clone(),
                        display_name: display_name.clone(),
                        avatar_url: avatar_url.clone(),
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

    let profile = account.fetch_user_profile_of(&user_id).await?;
    let display_name = profile.get_static::<DisplayName>()?;
    let avatar_url = profile.get_static::<AvatarUrl>()?;

    let custom_properties = get_custom_fields(&client, user_id.clone(), None).await;

    let profile = UserProfile {
        user_id,
        display_name,
        avatar_url,
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
