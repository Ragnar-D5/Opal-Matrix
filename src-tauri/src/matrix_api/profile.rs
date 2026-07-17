use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use matrix_sdk::{
    Client, Room, RoomMemberships,
    event_handler::Ctx,
    ruma::{
        OwnedUserId,
        events::room::member::OriginalSyncRoomMemberEvent,
        profile::{ProfileFieldName, ProfileFieldValue},
    },
};
use shared::{
    api::events::PresenceUpdate,
    profile::{CustomProperties, UserProfile},
};
use tauri::{AppHandle, State, command};
use tokio::sync::RwLock;

use crate::{TauriError, send_event};

fn banner_color_field() -> ProfileFieldName {
    "org.opal-matrix.banner_color".into()
}
fn name_color_field() -> ProfileFieldName {
    "org.opal-matrix.name_color".into()
}
fn sonic_signature_field() -> ProfileFieldName {
    "org.opal-matrix.sonic_signature".into()
}

pub fn send_user_profile_update(handle: &AppHandle, update: UserProfile) -> Result<(), TauriError> {
    send_event(handle, &update);
    Ok(())
}

pub async fn send_user_to_frontend(handle: &AppHandle, client: &Client) -> Result<(), TauriError> {
    let user_id = client.user_id().ok_or("Not logged in")?;
    let account = client.account();

    let (display_name_result, avatar_result, custom_properties) = tokio::join!(
        account.get_display_name(),
        account.get_avatar_url(),
        get_custom_fields(client, user_id.to_owned(), None),
    );

    let display_name = display_name_result.ok().flatten();

    let update = UserProfile {
        display_name,
        user_id: user_id.to_owned(),

        avatar_url: avatar_result.ok().flatten(),

        custom_properties,
    };
    send_user_profile_update(handle, update)?;

    Ok(())
}

pub async fn client_user_profile_event_handle(
    event: OriginalSyncRoomMemberEvent,
    handle: Ctx<AppHandle>,
    client: Client,
    debounce: Ctx<Arc<Mutex<ProfileDebounce>>>,
) {
    let Some(own_id) = client.user_id().map(|i| i.to_string()) else {
        log::error!("Received profile event but client has no user ID");
        return;
    };

    if event.state_key.as_str() != own_id {
        return;
    }

    if let Some(prev) = event.prev_content() {
        if prev.membership != event.content.membership {
            return;
        }
        if prev.displayname == event.content.displayname
            && prev.avatar_url == event.content.avatar_url
        {
            return;
        }
    }

    {
        let mut state = debounce.lock().unwrap();
        if state.timer_running {
            state.pending = true;
            return;
        }
        state.timer_running = true;
    }

    if let Err(e) = send_user_to_frontend(&handle, &client).await {
        log::error!("Failed to send user profile update: {e:?}");
    }

    let debounce_clone = debounce.clone();
    let handle_clone = handle.clone();
    let client_clone = client.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let should_emit = {
            let mut state = debounce_clone.lock().unwrap();
            let pending = state.pending;
            state.pending = false;
            state.timer_running = false;
            pending
        };

        if should_emit && let Err(e) = send_user_to_frontend(&handle_clone, &client_clone).await {
            log::error!("Failed to send user profile update after debounce: {e:?}");
        }
    });
}

#[derive(Debug, Default)]
pub struct ProfileDebounce {
    pending: bool,
    timer_running: bool,
}

/// Fetches custom profile fields (banner/name color, sonic signature), falling
/// back to `fallback` per-field when a field is unset or its fetch fails.
///
/// Some homeservers respond to the single-field profile endpoint with a
/// literal JSON `null` for a field that was never set, instead of omitting
/// the key as the spec expects; ruma fails to deserialize that, so this
/// treats "unset" and "the request errored" identically. Passing the
/// previously-resolved `CustomProperties` as `fallback` (instead of `None`)
/// means a flaky/unsupported request can't reset an already-known custom
/// color back to the user-id-derived default; only the first-ever resolution
/// for a user (no fallback available) does that.
pub async fn get_custom_fields(
    client: &Client,
    user_id: OwnedUserId,
    fallback: Option<CustomProperties>,
) -> CustomProperties {
    let account = client.account();
    let fallback = fallback.unwrap_or_else(|| CustomProperties::from_user_id(&user_id));

    let (banner_result, name_result, sonic_result) = tokio::join!(
        account.fetch_profile_field_of(user_id.clone(), banner_color_field()),
        account.fetch_profile_field_of(user_id.clone(), name_color_field()),
        account.fetch_profile_field_of(user_id.clone(), sonic_signature_field()),
    );

    let banner_color = banner_result
        .ok()
        .flatten()
        .map(|v| v.value().to_string())
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or(fallback.banner_color);

    let name_color = name_result
        .ok()
        .flatten()
        .map(|v| v.value().to_string())
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or(fallback.name_color);

    let sonic_signature = sonic_result
        .ok()
        .flatten()
        .map(|v| v.value().to_string())
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or(fallback.sonic_signature);

    CustomProperties {
        banner_color,
        name_color,
        sonic_signature,
    }
}

#[command]
pub async fn save_displayname(
    client: State<'_, RwLock<Client>>,
    name: String,
    room_id: Option<String>,
) -> Result<(), TauriError> {
    log::debug!("Saving display name: '{}' for room_id: {:?}", name, room_id);
    if let Some(room_id) = room_id {
        let room_id = matrix_sdk::ruma::RoomId::parse(room_id)?;

        let room = client
            .read()
            .await
            .get_room(&room_id)
            .ok_or("Not in room")?;
        room.set_own_member_display_name(Some(name)).await?;
        return Ok(());
    } else {
        client
            .read()
            .await
            .account()
            .set_display_name(Some(&name))
            .await?;
    }

    println!("Hosts: {:?}", cpal::platform::ALL_HOSTS);

    Ok(())
}

// #[command]
// pub async fn update_avatar_url(client: Client, path: Option<String>) -> Result<(), TauriError> {
//     let mxc_url = if let Some(path) = path {
//         let upload_response = client.upload(path.as_ref(), None).await?;
//         Some(upload_response.content_uri)
//     } else {
//         None
//     };
//     client.account().set_avatar_url(mxc_url.as_deref()).await?;
//     Ok(())
// }

#[command]
pub async fn save_namecolor(
    client: State<'_, RwLock<Client>>,
    handle: AppHandle,
    color: String,
) -> Result<(), TauriError> {
    log::debug!("Saving name color: '{}'", color);
    let value = serde_json::to_value(color)?;

    let profile_field = ProfileFieldValue::new(name_color_field().as_str(), value)?;

    let client = client.read().await;
    client.account().set_profile_field(profile_field).await?;
    send_user_to_frontend(&handle, &client).await?;

    Ok(())
}

#[command]
pub async fn save_bannercolor(
    client: State<'_, RwLock<Client>>,
    handle: AppHandle,
    color: String,
) -> Result<(), TauriError> {
    log::debug!("Saving banner color: '{}'", color);
    let value = serde_json::to_value(color)?;

    let profile_field = ProfileFieldValue::new(banner_color_field().as_str(), value)?;

    let client = client.read().await;
    client.account().set_profile_field(profile_field).await?;
    send_user_to_frontend(&handle, &client).await?;

    Ok(())
}

#[command]
pub async fn save_sonic_signature(
    client: State<'_, RwLock<Client>>,
    handle: AppHandle,
    signature: String,
) -> Result<(), TauriError> {
    let value = serde_json::to_value(signature)?;

    let profile_field = ProfileFieldValue::new(sonic_signature_field().as_str(), value)?;

    let client = client.read().await;
    client.account().set_profile_field(profile_field).await?;
    send_user_to_frontend(&handle, &client).await?;

    Ok(())
}

pub async fn send_presences(client: &Client, rooms: &[Room], handle: &AppHandle) {
    let mut user_ids: HashSet<OwnedUserId> = HashSet::new();

    for room in rooms {
        let members = match room.members(RoomMemberships::all()).await {
            Ok(members) => members,
            Err(e) => {
                log::warn!("Failed to get members for room: {}", e);
                continue;
            }
        };
        let ids: HashSet<OwnedUserId> = members.iter().map(|m| m.user_id().to_owned()).collect();
        user_ids.extend(ids);
    }

    let user_ids: Vec<OwnedUserId> = user_ids.into_iter().collect();
    let presences = match client.state_store().get_presence_events(&user_ids).await {
        Ok(presences) => presences,
        Err(e) => {
            log::warn!("Failed to get presence events: {}", e);
            return;
        }
    };

    let presence_batch: PresenceUpdate = presences
        .iter()
        .filter_map(|raw| {
            raw.deserialize()
                .map_err(|e| log::error!("Failed to deserialize a presence event: {e}"))
                .map(|event| (event.sender, event.content.into()))
                .ok()
        })
        .collect();

    if !presence_batch.is_empty() {
        send_event(handle, &presence_batch);
    }
}
