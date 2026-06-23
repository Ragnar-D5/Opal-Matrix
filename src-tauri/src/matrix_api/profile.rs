use std::sync::{Arc, Mutex};

use matrix_sdk::{
    Client,
    event_handler::Ctx,
    ruma::{
        OwnedUserId,
        events::room::member::OriginalSyncRoomMemberEvent,
        profile::{ProfileFieldName, ProfileFieldValue},
    },
};
use shared::profile::{CustomProperties, UserProfile};
use tauri::{AppHandle, Emitter, State, command};
use tokio::sync::RwLock;

use crate::TauriError;

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
    handle
        .emit("user_profile", update)
        .map_err(|e| format!("Failed to send user profile update: {e}").into())
}

pub async fn send_user_to_frontend(handle: &AppHandle, client: &Client) -> Result<(), TauriError> {
    let user_id = client.user_id().ok_or("Not logged in")?;
    let display_name = client.account().get_display_name().await?;
    let has_avatar = client.account().get_avatar_url().await?.is_some();

    let update = UserProfile {
        display_name,
        user_id: user_id.to_string(),

        has_avatar,

        custom_properties: get_custom_fields(client, user_id.to_owned()).await,
    };
    send_user_profile_update(handle, update)
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

pub async fn get_custom_fields(client: &Client, user_id: OwnedUserId) -> CustomProperties {
    let account = client.account();
    let derived = CustomProperties::from_user_id(user_id.as_str());

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
        .unwrap_or(derived.banner_color);

    let name_color = name_result
        .ok()
        .flatten()
        .map(|v| v.value().to_string())
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or(derived.name_color);

    let sonic_signature = sonic_result
        .ok()
        .flatten()
        .map(|v| v.value().to_string())
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or(derived.sonic_signature);

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
