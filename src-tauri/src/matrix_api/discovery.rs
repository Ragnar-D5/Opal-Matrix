use std::collections::HashMap;

use anyhow::Context;
use serde::Deserialize;

use crate::TauriError;

#[derive(Debug, Deserialize)]
pub struct WellKnown {
    #[serde(rename = "m.homeserver")]
    pub homeserver: HomeServer,
    // #[serde(rename = "org.matrix.msc2965.authentication")]
    // pub authentication: Authentication,
}

#[derive(Debug, Deserialize)]
pub struct HomeServer {
    #[serde(rename = "base_url")]
    pub base_url: String,
}

// #[derive(Debug, Deserialize)]
// pub struct Authentication {
//     #[serde(rename = "issuer")]
//     pub authentication_url: String,

//     #[serde(rename = "account")]
//     pub account_url: String,
// }

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
#[tauri::command]
pub async fn choose_home_server(url: String) -> Result<String, (String, TauriError)> {
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
            .context("Failed to parse .well-known response")
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
