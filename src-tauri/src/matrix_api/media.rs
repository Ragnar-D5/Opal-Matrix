use matrix_sdk::{
    Client,
    media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
    ruma::{OwnedRoomId, OwnedUserId, api::client::profile::AvatarUrl, events::room::MediaSource},
};
use tauri::{State, command};
use tokio::sync::RwLock;

use crate::TauriError;

async fn fetch_media(
    client: &Client,
    source: MediaSource,
    format: MediaFormat,
) -> Result<tauri::ipc::Response, TauriError> {
    let parameters = MediaRequestParameters { source, format };
    let media = client.media().get_media_content(&parameters, true).await?;

    Ok(tauri::ipc::Response::new(media))
}

#[command(rename_all = "snake_case")]
pub async fn get_file(
    client: State<'_, RwLock<Client>>,
    source: MediaSource,
) -> Result<tauri::ipc::Response, TauriError> {
    let client = client.read().await;
    fetch_media(&client, source, MediaFormat::File).await
}

#[command(rename_all = "snake_case")]
pub async fn get_thumbnail(
    client: State<'_, RwLock<Client>>,
    source: MediaSource,
    settings: MediaThumbnailSettings,
) -> Result<tauri::ipc::Response, TauriError> {
    let client = client.read().await;
    fetch_media(&client, source, MediaFormat::Thumbnail(settings)).await
}

#[command(rename_all = "snake_case")]
pub async fn get_user_avatar(
    client: State<'_, RwLock<Client>>,
    user_id: OwnedUserId,
) -> Result<tauri::ipc::Response, TauriError> {
    let client = client.read().await;

    let profile = client.account().fetch_user_profile_of(&user_id).await?;

    let avatar_url = profile
        .get_static::<AvatarUrl>()?
        .ok_or("No avatar available")?;

    fetch_media(&client, MediaSource::Plain(avatar_url), MediaFormat::File).await
}

#[command(rename_all = "snake_case")]
pub async fn get_room_avatar(
    client: State<'_, RwLock<Client>>,
    room_id: OwnedRoomId,
) -> Result<tauri::ipc::Response, TauriError> {
    let client = client.read().await;

    let room = client
        .get_room(&room_id)
        .ok_or(format!("Room {room_id} not found"))?;
    let bytes = room
        .avatar(MediaFormat::File)
        .await?
        .ok_or("No avatar available")?;

    Ok(tauri::ipc::Response::new(bytes))
}

#[command(rename_all = "snake_case")]
pub async fn get_member_avatar(
    client: State<'_, RwLock<Client>>,
    room_id: OwnedRoomId,
    user_id: OwnedUserId,
) -> Result<tauri::ipc::Response, TauriError> {
    let client = client.read().await;

    let room = client.get_room(&room_id).ok_or("No room available")?;
    let member = room
        .get_member(&user_id)
        .await?
        .ok_or(format!("Member {user_id} not found in room {room_id}"))?;
    let bytes = member
        .avatar(MediaFormat::File)
        .await?
        .ok_or("No avatar available")?;

    Ok(tauri::ipc::Response::new(bytes))
}
