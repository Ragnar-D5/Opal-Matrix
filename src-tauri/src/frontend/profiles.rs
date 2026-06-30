use std::collections::HashMap;

use futures::future::join_all;
use matrix_sdk::{
    event_handler::Ctx,
    ruma::{
        events::{room::member::OriginalSyncRoomMemberEvent, typing::SyncTypingEvent},
        profile::ProfileFieldName,
        OwnedUserId, UserId,
    },
    Client, Room, RoomMemberships,
};
use shared::{
    profile::{CustomProperties, MemberProfile, UserProfile},
    synth::ProfileAudio,
};
use tauri::{async_runtime::spawn_blocking, command, AppHandle, Emitter, State};
use tokio::sync::RwLock;

use crate::{matrix_api::profile::get_custom_fields, TauriError};

pub async fn on_member_update(
    event: OriginalSyncRoomMemberEvent,
    room: Room,
    app_handle: Ctx<AppHandle>,
    default_audio: Ctx<ProfileAudio>,
) {
    let content = event.content;

    let (custom_properties, sonic_signature) =
        get_custom_fields(&room.client(), event.state_key.clone(), &default_audio).await;

    let profile = MemberProfile {
        room_id: room.room_id().to_string(),
        profile: UserProfile {
            user_id: event.state_key.to_string(),
            display_name: content.displayname,
            has_avatar: content.avatar_url.is_some(),

            custom_properties,
        },
    };

    let payload = HashMap::from([(room.room_id().to_string(), vec![profile.clone()])]);
    if let Err(e) = send_member_update(&app_handle, payload) {
        log::error!("Failed to send member update: {:?}", e);
    };

    let handle = app_handle.clone();
    let mut profile = profile.clone();

    spawn_blocking(move || {
        let audio = ProfileAudio::new(sonic_signature);

        profile.profile.custom_properties.audio = audio;
        let payload = HashMap::from([(room.room_id().to_string(), vec![profile])]);
        if let Err(e) = send_member_update(&handle, payload) {
            log::error!("Failed to send member update with audio: {:?}", e);
        }
    });
}

pub async fn send_all_members(
    client: &Client,
    handle: &AppHandle,
    rooms: &[Room],
    default_audio: &ProfileAudio,
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

                user_memberships.entry(user_id.clone()).or_default().push((
                    room_id.clone(),
                    has_avatar,
                    display_name.clone(),
                ));

                MemberProfile {
                    room_id: room_id.clone(),
                    profile: UserProfile {
                        user_id: user_id.to_string(),
                        display_name,
                        has_avatar,
                        custom_properties: CustomProperties::from_user_id(
                            user_id.as_str(),
                            default_audio.clone(),
                        ),
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
                let props = get_custom_fields(&client, user_id.clone(), default_audio).await;
                (user_id, props)
            }
        })
        .collect();

    let results = join_all(futs).await;

    let mut update_payload: HashMap<String, Vec<MemberProfile>> = HashMap::new();
    for (user_id, (custom_properties, _)) in &results {
        for (room_id, has_avatar, display_name) in &user_memberships[user_id] {
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

    send_member_update(handle, update_payload.clone())?;

    update_payload.clear();
    for (user_id, (custom_properties, sonic_signature)) in results {
        let audio = ProfileAudio::new(sonic_signature);
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
                        custom_properties: CustomProperties {
                            audio: audio.clone(),
                            ..custom_properties.clone()
                        },
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
    client: State<'_, RwLock<Client>>,
    default_audio: State<'_, ProfileAudio>,
    handle: AppHandle,
) -> Result<UserProfile, TauriError> {
    let client = client.read().await;

    let user_id = UserId::parse(user_id)?;

    let display_name = client
        .account()
        .fetch_profile_field_of(user_id.clone(), ProfileFieldName::DisplayName)
        .await?
        .and_then(|v| v.value().as_str().map(|t| t.to_string()));

    let has_avatar = client
        .account()
        .fetch_profile_field_of(user_id.clone(), ProfileFieldName::AvatarUrl)
        .await?
        .is_some();

    let (custom_properties, sonic_signature) =
        get_custom_fields(&client, user_id.clone(), &default_audio).await;

    let profile = UserProfile {
        user_id: user_id.to_string(),
        display_name,
        has_avatar,
        custom_properties,
    };

    {
        let handle = handle.clone();
        let mut profile = profile.clone();

        spawn_blocking(move || {
            let audio = ProfileAudio::new(sonic_signature);

            profile.custom_properties.audio = audio;

            if let Err(e) = handle.emit("user_profile", profile.clone()) {
                log::error!("Failed to send user profile update with audio: {:?}", e);
            }
        });
    }

    Ok(profile)
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
