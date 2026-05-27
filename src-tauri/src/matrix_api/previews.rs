use matrix_sdk::Client as MatrixClient;
use matrix_sdk::ruma::api::client::authenticated_media::get_media_preview::v1::Request as GetMediaPreviewRequest;
use regex::Regex;
use shared::api::LinkPreviewResponse;
use tauri::{State, command};
use tokio::sync::RwLock;

use crate::{BrandColorsMap, TauriError};

/// Tauri command to get a URL preview for a given URL.
#[command]
pub async fn get_url_preview(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    color_map: State<'_, BrandColorsMap>,
    url: String,
) -> Result<LinkPreviewResponse, TauriError> {
    let reddit_replacement = Regex::new(r"(?i)(https?://)?(?:[a-z0-9-]+.)*\breddit.com\b").unwrap();

    let url = reddit_replacement.replace_all(&url, "${1}vxreddit.com");

    let matrix_client = matrix_client.read().await;

    let request = GetMediaPreviewRequest::new(url.to_string());
    let response = matrix_client.send(request).await?;

    let Some(data) = response.data else {
        return Err("No data in response".into());
    };

    let mut preview: LinkPreviewResponse = serde_json::from_str(data.get())?;
    preview.resolve_color(&url, &color_map.0);

    Ok(preview)
}
