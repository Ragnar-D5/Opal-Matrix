use anyhow::{anyhow, Context};
use serde::Deserialize;

use crate::AppState;

#[derive(Debug, Deserialize)]
struct MatrixConfig {
    #[serde(rename = "m.homeserver")]
    pub homeserver: HomeServer,
}

#[derive(Debug, Deserialize)]
struct HomeServer {
    #[serde(rename = "base_url")]
    pub base_url: String,
}

pub(crate) async fn get_well_known(_state: &AppState, website: String) -> anyhow::Result<String> {
    let client = reqwest::Client::new();

    let res = client
        .get(format!("https://{website}/.well-known/matrix/client"))
        .send()
        .await
        .context("Failed to get /.well-known/matrix/client")?;

    if res.status().is_success() {
        let well_known: MatrixConfig = res
            .json()
            .await
            .context("Failed to parse .well-known response")?;
        dbg!(&well_known.homeserver.base_url);
        Ok(well_known.homeserver.base_url)
    } else {
        let content: MatrixConfig = res.json().await?;
        dbg!(&content);
        return Err(anyhow!(format!(
            "GET request to /.well-known/matrix/client resulted in an error: {:?}",
            content
        )));
    }
}
