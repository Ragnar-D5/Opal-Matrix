use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use log::info;
use ruma::api::auth_scheme::SendAccessToken;
use serde::Deserialize;
use tauri::State;
use tauri_plugin_http::reqwest::{self, Client};

use crate::state::HomeServerInfo;
use crate::{AppState, TauriError, create_http_response};

#[derive(Debug, Deserialize)]
pub struct WellKnown {
    #[serde(rename = "m.homeserver")]
    pub homeserver: HomeServer,
    #[serde(rename = "org.matrix.msc2965.authentication")]
    pub authentication: Authentication,
}

#[derive(Debug, Deserialize)]
pub struct HomeServer {
    #[serde(rename = "base_url")]
    pub base_url: String,
}

/// Handles the necessary authentication configuration for the server
#[derive(Debug, Deserialize)]
pub struct Authentication {
    #[serde(rename = "issuer")]
    pub authentication_url: String,

    #[serde(rename = "account")]
    pub account_url: String,
}

// TODO: Implement checking for supported versions and unstable features to
// find out if the server supports the required features for the app to work
//
// Right now this is just used to check wether a server actually lives at the specified url
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct VersionsResponseErik {
    pub versions: Vec<String>,

    pub unstable_features: HashMap<String, bool>,
}

/// Checks wether a .well-known/matrix/client file lies at the specified url and if it can be parsed
///
/// This function is invoked by the frontend with a potential url of a host server and
/// returns `Ok(url)` if a working server lives there and `Err((url, TauriError))` if
/// some error happend while checking.
///
/// This function is meant to be called repeatedly with different urls until a working server is found, which
/// is indicated by a successful response and the valid homeserver url.
#[tauri::command]
pub async fn try_home_server(url: String) -> Result<String, (String, TauriError)> {
    let client = reqwest::Client::new();

    let res = client
        .get(format!("https://{url}/.well-known/matrix/client"))
        .send()
        .await
        .context(format!(
            "Error while sending GET request to https://{url}/.well-known/matrix/client"
        ))
        .map_err(|e| (url.clone(), e.into()))?;

    if res.status().is_success() {
        let well_known: WellKnown = res
            .json()
            .await
            .context("Failed to parse .well-known response") // TODO: Change how this is handled, since right now the error might originate from the server not supporting oidc or the json being malformed
            .map_err(|e| (url.clone(), e.into()))?;
        let ver = client
            .get(format!(
                "{}/_matrix/client/versions",
                well_known.homeserver.base_url
            ))
            .send()
            .await
            .context(format!(
                "Error while sending Get request to {}",
                well_known.homeserver.base_url
            ))
            .map_err(|e| (url.clone(), e.into()))?;
        let _parse_test: VersionsResponseErik = ver.json().await.context("Failed to parse response from /_matrix/client/versions. Does a Matrix server live here?").map_err(|e| (url.clone(), e.into()))?;
        Ok(url)
    } else {
        return Err((
            url.clone(),
            format!(
                "GET request to /.well-known/matrix/client resulted in an error: {:?}",
                res
            )
            .into(),
        ));
    }
}

// TODO: Refactor this function alongside `try_home_server` and the frontend to be more robust and less verbose.
/// Choose a home server
///
/// This is largely duplicate code from `try_home_server`, but is used to actually alter the state of the app.
#[tauri::command]
pub async fn choose_home_server(
    state: State<'_, Arc<AppState>>,
    url: String,
) -> Result<String, (String, TauriError)> {
    let client = reqwest::Client::new();
    info!("Setting matrix_url to {url}");

    let res = client
        .get(format!("https://{url}/.well-known/matrix/client"))
        .send()
        .await
        .context(format!(
            "Error while sending GET request to https://{url}/.well-known/matrix/client"
        ))
        .map_err(|e| (url.clone(), e.into()))?;

    if res.status().is_success() {
        let well_known: WellKnown = res
            .json()
            .await
            .context("Failed to parse .well-known response") // TODO: Change how this is handled, since right now the error might originate from the server not supporting oidc or the json being malformed
            .map_err(|e| (url.clone(), e.into()))?;

        *state.home_server_info.write().await = Some(
            HomeServerInfo::try_new(well_known.homeserver.base_url)
                .await
                .map_err(|e| (url.clone(), e.into()))?,
        );
        *state.auth_provider.write().await = Some(well_known.authentication);
        Ok(url)
    } else {
        return Err((
            url.clone(),
            format!(
                "GET request to /.well-known/matrix/client resulted in an error: {:?}",
                res
            )
            .into(),
        ));
    }
}
use ruma::api::client::discovery::get_supported_versions::{
    Request as VersionsRequest, Response as VersionsResponse,
};
use ruma::api::{IncomingResponse, OutgoingRequest, SupportedVersions};
pub async fn fetch_supported_versions(base_url: String) -> Result<SupportedVersions, TauriError> {
    let req = VersionsRequest::new().try_into_http_request::<Vec<u8>>(
        base_url.as_str(),
        SendAccessToken::None,
        (),
    )?;

    let http_req = reqwest::Request::try_from(req)?;

    let res = create_http_response(Client::new().execute(http_req).await?)
        .await
        .map_err(|e| format!("Failed to create HTTP response: {:?}", e))?;

    Ok(VersionsResponse::try_from_http_response(res)?.as_supported_versions())
}
