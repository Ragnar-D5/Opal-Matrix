use matrix_sdk::{
    Client,
    ruma::{
        api::client::authenticated_media::get_media_preview::v1::Request as GetMediaPreviewRequest,
        events::room::message::UrlPreview,
    },
};
use regex::Regex;
use shared::api::LinkPreviewResponse;
use tauri::{State, command};
use tokio::sync::RwLock;
use url::Url;

use crate::LogResultExt;
use crate::{BrandColorsMap, TauriError};

/// Tauri command to get a URL preview for a given URL.
#[command]
pub async fn get_url_preview(
    matrix_client: State<'_, RwLock<Client>>,
    color_map: State<'_, BrandColorsMap>,
    url: String,
) -> Result<LinkPreviewResponse, TauriError> {
    let reddit_replacement = Regex::new(r"(?i)(https?://)?(?:[a-z0-9-]+.)*\breddit.com\b").unwrap();

    let url = reddit_replacement.replace_all(&url, "${1}vxreddit.com");

    let matrix_client = matrix_client.read().await;

    let request = GetMediaPreviewRequest::new(url.to_string());
    let response = matrix_client.send(request).await.log_as_debug()?;

    let Some(data) = response.data else {
        return Err("No data in response".into());
    };

    let mut preview: LinkPreviewResponse = serde_json::from_str(data.get())?;
    preview.resolve_color(&url, &color_map.0);

    if preview.title.is_empty() {
        preview.title = preview
            .site_name
            .clone()
            .or_else(|| preview.url.clone())
            .unwrap_or_else(|| url.to_string());
    }

    Ok(preview)
}

async fn get_link_preview(client: &Client, url: &Url) -> Result<Option<UrlPreview>, TauriError> {
    let response = client
        .send(GetMediaPreviewRequest::new(url.to_string()))
        .await
        .log_as_debug()?;

    let Some(data) = response.data else {
        return Ok(None);
    };

    let mut preview: UrlPreview = serde_json::from_str(data.get())?;
    preview.matched_url = Some(url.to_string());

    Ok(Some(preview))
}

pub async fn get_link_previews(client: &Client, urls: &[Url]) -> Option<Vec<UrlPreview>> {
    let previews: Vec<UrlPreview> =
        futures::future::join_all(urls.iter().map(|url| get_link_preview(client, url)))
            .await
            .into_iter()
            .filter_map(|res| res.ok().flatten())
            .collect::<Vec<_>>();

    (!previews.is_empty()).then_some(previews)
}
