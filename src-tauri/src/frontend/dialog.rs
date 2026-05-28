use tauri::{AppHandle, command};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;

use crate::TauriError;

#[command(rename_all = "snake_case")]
pub async fn open_file_dialog(app: AppHandle) -> Result<Option<String>, TauriError> {
    let (tx, rx) = oneshot::channel();

    app.dialog()
        .file()
        .set_title("Select a file")
        .pick_file(move |result| {
            let _ = tx.send(result);
        });

    let file_path = rx.await?;

    log::debug!("File dialog result: {:?}", file_path);

    Ok(file_path.map(|p| p.to_string()))
}
