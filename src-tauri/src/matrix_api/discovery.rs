use std::collections::HashMap;

use anyhow::Context;
use serde::Deserialize;
use tauri::State;
use tauri_plugin_http::reqwest;

use crate::{AppState, TauriError};

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
pub struct VersionsResponse {
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
        let _parse_test: VersionsResponse = ver.json().await.context("Failed to parse response from /_matrix/client/versions. Does a Matrix server live here?").map_err(|e| (url.clone(), e.into()))?;
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
    url: String,
    state: State<'_, AppState>,
) -> Result<String, (String, TauriError)> {
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
        *state.matrix_url.write().await = Some(well_known.homeserver.base_url);
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
