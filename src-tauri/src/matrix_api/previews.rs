use matrix_sdk::ruma::OwnedMxcUri;
use matrix_sdk::ruma::{
    api::client::authenticated_media::get_media_preview::v1::Request as GetMediaPreviewRequest,
    events::room::MediaSource,
};
use matrix_sdk::Client as MatrixClient;
use regex::Regex;
use shared::api::LinkPreviewResponse;
use tauri::{command, State};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{state::MediaManager, BrandColorsMap, TauriError};

/// Tauri command to get a URL preview for a given URL.
#[command]
pub async fn get_url_preview(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    color_map: State<'_, BrandColorsMap>,
    meia_manager: State<'_, MediaManager>,
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

    if let Some(ref url) = preview.image_url.clone() {
        let media_id = Uuid::new_v4();
        let image_url = OwnedMxcUri::from(url.as_str());
        meia_manager
            .sources
            .write()
            .await
            .insert(media_id, MediaSource::Plain(image_url));
        preview.image_url = Some(format!("mxc://media/{}", media_id));
    }

    Ok(preview)
}
