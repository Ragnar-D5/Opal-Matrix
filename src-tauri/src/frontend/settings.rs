use tauri::{AppHandle, Manager, command};

use crate::TauriError;

#[command(rename_all = "snake_case")]
pub async fn change_screen_scaling(handle: AppHandle, scale_factor: f64) -> Result<(), TauriError> {
    let window = handle
        .get_webview_window("main")
        .ok_or("Couldn't get main window")?;
    window.set_zoom(scale_factor)?;
    Ok(())
}
