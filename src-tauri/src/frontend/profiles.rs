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
    api::events::TypingUpdate,
    profile::{CustomProperties, MemberProfile, UserProfile},
    synth::ProfileAudio,
};
use tauri::{async_runtime::spawn_blocking, command, AppHandle, State};
use tokio::sync::RwLock;

use crate::{matrix_api::profile::get_custom_fields, send_event, TauriError};

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
    send_event(&app_handle, &payload);

    let handle = app_handle.clone();
    let mut profile = profile.clone();

    spawn_blocking(move || {
        let audio = ProfileAudio::new(sonic_signature);

        profile.profile.custom_properties.audio = audio;
        let payload = HashMap::from([(room.room_id().to_string(), vec![profile])]);
        send_event(&handle, &payload);
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
        let members = room.members(RoomMemberships::all()).await?;

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

    send_event(handle, &payload);

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

    send_event(handle, &update_payload);

    // Render everyone's audio on a single background thread instead of the
    // async runtime (keeps it off the tokio workers) and instead of one
    // spawn_blocking per member (spawning a burst of OS threads is itself
    // slow), then deliver it as a single batch.
    let update_payload = spawn_blocking(move || {
        let mut update_payload: HashMap<String, Vec<MemberProfile>> = HashMap::new();
        for (user_id, (custom_properties, sonic_signature)) in results {
            let custom_properties = CustomProperties {
                audio: ProfileAudio::new(sonic_signature),
                ..custom_properties
            };
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
        update_payload
    })
    .await
    .expect("audio render task panicked");

    send_event(handle, &update_payload);

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

            send_event(&handle, &profile);
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

    send_event(
        &handle,
        &TypingUpdate {
            room_id: room_id.clone(),
            user_ids: user_ids.clone(),
        },
    );
}
