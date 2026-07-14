use tauri::command;
use tauri_plugin_http::reqwest;

use crate::TauriError;

const VERSIONS_URL: &str = "https://download.erik-is.gay/release-notes";

#[command]
pub async fn get_versions() -> Result<Vec<String>, TauriError> {
    let response = reqwest::get(VERSIONS_URL).await?;

    let json: Vec<String> = response.json().await?;
    Ok(json)
}

#[command]
pub async fn get_version(version: String) -> Result<String, TauriError> {
    let response = reqwest::get(format!("{}?version={}", VERSIONS_URL, version)).await?;

    let text = response.text().await?;
    Ok(text)
}
