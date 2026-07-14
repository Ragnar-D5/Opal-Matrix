use tauri::{AppHandle, Manager, command};

use crate::TauriError;

// A degenerate zoom (e.g. from a hand-edited or corrupted settings file) can make
// WebKitGTK/the webview's GPU-backed surfaces resize to an absurd size and destabilize
// the GPU driver, so the factor actually applied to the window is always clamped here,
// regardless of where it came from.
const MIN_SCALE_FACTOR: f64 = 0.25;
const MAX_SCALE_FACTOR: f64 = 3.0;

#[command(rename_all = "snake_case")]
pub async fn change_screen_scaling(handle: AppHandle, scale_factor: f64) -> Result<(), TauriError> {
    let scale_factor = scale_factor.clamp(MIN_SCALE_FACTOR, MAX_SCALE_FACTOR);
    log::info!("change_screen_scaling: scale_factor={scale_factor}");
    let window = handle
        .get_webview_window("main")
        .ok_or("Couldn't get main window")?;
    window.set_zoom(scale_factor)?;
    Ok(())
}
