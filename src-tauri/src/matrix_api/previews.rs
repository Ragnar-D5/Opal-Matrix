use regex::Regex;
use ruma::api::{
    IncomingResponse, OutgoingRequest,
    client::authenticated_media::get_media_preview::v1::{
        Request as GetMediaPreviewRequest, Response as GetMediaPreviewResponse,
    },
};
use shared::api::LinkPreviewResponse;
use std::{borrow::Cow, sync::Arc};
use tauri::{State, command};
use tauri_plugin_http::reqwest;
use url::Url;

use crate::{
    BrandColorsMap, TauriError, reqwest_response_to_http_response,
    state::{AppState, HomeServerInfo},
};

/// Fetches a URL preview for a given URL and timestamp.
async fn fetch_url_preview(
    server_info: &HomeServerInfo,
    token: &String,
    url: &String,
) -> Result<LinkPreviewResponse, TauriError> {
    let reddit_replacement =
        Regex::new(r"(?i)(https?://)?(?:[a-z0-9-]+\.)*\breddit\.com\b").unwrap();
    let url = reddit_replacement.replace_all(url, "${1}vxreddit.com");
    log::debug!("{url}");

    let req = GetMediaPreviewRequest::new(url.to_string()).try_into_http_request::<Vec<u8>>(
        server_info.base_url.as_str(),
        ruma::api::auth_scheme::SendAccessToken::Always(token.as_str()),
        Cow::Borrowed(&server_info.supported_versions),
    )?;

    let http_req = reqwest::Request::try_from(req)?;

    let res =
        reqwest_response_to_http_response(reqwest::Client::new().execute(http_req).await?).await?;

    let preview_res = GetMediaPreviewResponse::try_from_http_response(res)?;

    let preview: LinkPreviewResponse =
        serde_json::from_str(preview_res.data.unwrap_or_default().get())?;

    Ok(preview)
}

/// Tauri command to get a URL preview for a given URL.
#[command]
pub async fn get_url_preview(
    state: State<'_, Arc<AppState>>,
    color_map: State<'_, BrandColorsMap>,
    url: String,
) -> Result<LinkPreviewResponse, TauriError> {
    let (token, server_info) = state.get_api().await?;

    let mut res = fetch_url_preview(&server_info, &token, &url).await?;
    res.resolve_color(&url, color_map.0.clone());

    Ok(res)
}
