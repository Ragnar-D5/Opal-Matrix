use matrix_sdk::Client as MatrixClient;

use log::trace;
use matrix_sdk::ruma::api::{
    auth_scheme::SendAccessToken,
    client::discovery::get_supported_versions::{
        Request as VersionsRequest, Response as VersionsResponse,
    },
    IncomingResponse, OutgoingRequest, SupportedVersions,
};
use serde::Deserialize;
use tauri::State;
use tauri_plugin_http::reqwest::{self, Client};
use tokio::sync::RwLock;
use url::Url;

use crate::{reqwest_response_to_http_response, AsInfo, TauriError};

#[derive(Debug, Deserialize)]
pub struct WellKnown {
    #[serde(rename = "m.homeserver")]
    pub homeserver: HomeServer,
}

#[derive(Debug, Deserialize)]
pub struct HomeServer {
    #[serde(rename = "base_url")]
    pub base_url: String,
}

// TODO:
/// Handles the necessary authentication configuration for the server
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Authentication {
    #[serde(rename = "issuer")]
    pub authentication_url: String,

    #[serde(rename = "account")]
    pub account_url: String,
}

/// Choose a home server
///
/// This is largely duplicate code from `try_home_server`, but is used to actually alter the state of the app.
#[tauri::command]
pub async fn choose_home_server(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    url: String,
) -> Result<String, TauriError> {
    let client = reqwest::Client::new();
    trace!("Trying to set matrix_url to {url}");

    let res = client
        .get(format!("https://{url}/.well-known/matrix/client"))
        .send()
        .await
        .map_err(|e| format!("Failed to get .well_known: {e}").as_info())?;

    if res.status().is_success() {
        let well_known: WellKnown = res.json().await.map_err(|e| {
            format!("Failed to parse .well-known response (does the server support OIDC?): {e}",)
        })?;

        *matrix_client.write().await = MatrixClient::builder()
            .homeserver_url(
                Url::parse(&well_known.homeserver.base_url)
                    .map_err(|e| format!("Failed to parse homeserver URL: {e}").as_info())?,
            )
            .handle_refresh_tokens()
            .build()
            .await
            .map_err(|e| format!("Failed to build Matrix client: {e}").as_info())?;

        let _ = fetch_supported_versions(&well_known.homeserver.base_url).await?;
        Ok(url)
    } else {
        Err(format!("Failed to get .well_known: {}", res.status()).as_info())
    }
}

pub async fn fetch_supported_versions(base_url: &str) -> Result<SupportedVersions, TauriError> {
    let req = VersionsRequest::new().try_into_http_request::<Vec<u8>>(
        base_url,
        SendAccessToken::None,
        (),
    )?;

    let http_req = reqwest::Request::try_from(req)?;

    let res = reqwest_response_to_http_response(Client::new().execute(http_req).await?)
        .await
        .map_err(|e| format!("Failed to create HTTP response: {:?}", e))?;

    Ok(VersionsResponse::try_from_http_response(res)?.as_supported_versions())
}
