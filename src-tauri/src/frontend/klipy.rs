use klipy::{MediaItem, Page};
use tauri_plugin_http::reqwest;

use crate::TauriError;

/// Search klipy using the given string
#[tauri::command]
pub async fn search_gifs(search_term: String, page: u32) -> Result<Page<MediaItem>, TauriError> {
    let klipy = klipy::Klipy::builder(env!("KLIPY_API_KEY"))
        .http_client(reqwest::Client::new())
        .build();

    let page = klipy.gifs().search(search_term).page(page).send().await?;
    Ok(page)
}
