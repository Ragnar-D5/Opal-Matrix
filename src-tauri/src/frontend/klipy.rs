use klipy::{MediaItem, Page};
use tauri::{State, command};
use tauri_plugin_http::reqwest;
use tokio_util::sync::CancellationToken;

use crate::{TauriError, state::TaskManager};

/// Search klipy using the given string
#[command(rename_all = "snake_case")]
pub async fn search_gifs(
    search_term: String,
    page: u32,
    task_manger: State<'_, TaskManager>,
) -> Result<Page<MediaItem>, TauriError> {
    let token = CancellationToken::new();
    task_manger.replace_task("gif_search", token.clone()).await;

    tokio::select! {
        _ = token.cancelled() => {
            log::debug!("Gif fetch was cancelled by a newer request");
            Ok(Page { data: Vec::new(), current_page: None, per_page: None, has_next: false, meta: None })
        }
        result = async {
            let klipy = klipy::Klipy::builder("9dEuScySalHgQ4miP7P3q6lYhneUEHJbW2Yft54tfCwJ2uTox3QKGgn0BN2un4mG")
                .http_client(reqwest::Client::new())
                .build();

            let page = klipy.gifs().search(search_term).page(page).send().await?;
            Ok(page)
        } => result,
    }
}
