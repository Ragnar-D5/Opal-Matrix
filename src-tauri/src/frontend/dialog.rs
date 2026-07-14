use std::fs;

use matrix_sdk::Client;
use shared::{
    api::{FileMetadata, UiAttachmentSource},
    timeline::UiMediaSource,
};
use tauri::{AppHandle, State, command};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::RwLock;

use crate::{TauriError, matrix_api::media::get_media_bytes, state::MediaManager};

#[command(rename_all = "snake_case")]
pub async fn open_file_dialog(app: AppHandle) -> Result<Vec<FileMetadata>, TauriError> {
    let files = app
        .dialog()
        .file()
        .set_title("Select files")
        .blocking_pick_files();

    let paths = files
        .unwrap_or_default()
        .iter()
        .filter_map(|v| {
            let path = v.as_path()?;

            let file_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            let mime_type = mime_guess::from_path(path)
                .first_or_octet_stream()
                .essence_str()
                .to_string();

            let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            Some(FileMetadata {
                source: UiAttachmentSource::LocalFile(path.to_string_lossy().to_string()),
                file_name,
                mime_type,
                size,
            })
        })
        .collect();

    log::debug!("Paths selected: {:?}", paths);

    Ok(paths)
}

#[command(rename_all = "snake_case")]
pub async fn save_file_to_picked_dest(
    app: AppHandle,
    source: UiMediaSource,
    file_name: String,
    media_manager: State<'_, MediaManager>,
    client: State<'_, RwLock<Client>>,
) -> Result<(), TauriError> {
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Select destination")
        .set_file_name(file_name)
        .blocking_save_file()
    else {
        log::debug!("No destination selected");
        return Ok(());
    };

    let Some(path) = path.as_path() else {
        log::debug!("Invalid destination path");
        return Ok(());
    };

    let bytes = get_media_bytes(&client.read().await.clone(), source, &media_manager).await?;

    log::debug!("Destination selected: {}", path.to_string_lossy());

    fs::write(path, bytes)?;

    Ok(())
}
