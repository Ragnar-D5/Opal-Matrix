use std::fs;

use shared::api::FileMetadata;
use tauri::{command, AppHandle};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;

use crate::TauriError;

#[command(rename_all = "snake_case")]
pub async fn open_file_dialog(app: AppHandle) -> Result<Vec<FileMetadata>, TauriError> {
    let (tx, rx) = oneshot::channel();

    app.dialog()
        .file()
        .set_title("Select a file")
        .pick_files(move |result| {
            let _ = tx.send(result);
        });

    let files = rx.await?;

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
                path: path.to_string_lossy().into_owned(),
                file_name,
                mime_type,
                size,
            })
        })
        .collect();

    log::debug!("Paths selected: {:?}", paths);

    Ok(paths)
}
