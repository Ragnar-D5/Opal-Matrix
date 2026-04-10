use anyhow::{anyhow, Context};
use serde::Deserialize;

use crate::AppState;

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

#[derive(Debug, Deserialize)]
pub struct Authentication {
    #[serde(rename = "issuer")]
    pub authentication_url: String,

    #[serde(rename = "account")]
    pub account_url: String,
}

pub(crate) async fn get_well_known(
    _state: &AppState,
    website: String,
) -> anyhow::Result<WellKnown> {
    let client = reqwest::Client::new();

    let res = client
        .get(format!("https://{website}/.well-known/matrix/client"))
        .send()
        .await
        .context("Failed to get /.well-known/matrix/client")?;

    if res.status().is_success() {
        let well_known: WellKnown = res
            .json()
            .await
            .context("Failed to parse .well-known response")?;
        Ok(well_known)
    } else {
        let content: WellKnown = res.json().await?;
        return Err(anyhow!(format!(
            "GET request to /.well-known/matrix/client resulted in an error: {:?}",
            content
        )));
    }
}
