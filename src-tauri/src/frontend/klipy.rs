use klipy::{MediaItem, Page};
use tauri_plugin_http::reqwest;

use crate::TauriError;

/// Search klipy using the given string
#[tauri::command]
pub async fn search_gifs(search_term: String, page: u32) -> Result<Page<MediaItem>, TauriError> {
    let klipy =
        klipy::Klipy::builder("9dEuScySalHgQ4miP7P3q6lYhneUEHJbW2Yft54tfCwJ2uTox3QKGgn0BN2un4mG")
            .http_client(reqwest::Client::new())
            .build();

    let page = klipy.gifs().search(search_term).page(page).send().await?;
    Ok(page)
}
