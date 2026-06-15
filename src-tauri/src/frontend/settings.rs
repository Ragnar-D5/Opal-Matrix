use macros::matrix_settings;
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

#[matrix_settings]
#[derive(Debug)]
pub struct Settings {
    #[setting("Skalierung", false, default = 1.0)]
    pub scaling: f64,

    pub test: u32,
}
