use matrix_sdk::{
    Client,
    media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
    ruma::events::room::MediaSource,
};
use tauri::{State, command};
use tokio::sync::RwLock;

use crate::{LogResultExt, TauriError};

async fn fetch_media(
    client: &Client,
    source: MediaSource,
    format: MediaFormat,
) -> Result<tauri::ipc::Response, TauriError> {
    let parameters = MediaRequestParameters { source, format };
    let media = client
        .media()
        .get_media_content(&parameters, true)
        .await
        .log_as_debug()?;

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
