use anyhow::{anyhow, Context};
use serde::Deserialize;
use version_compare::Version;

#[derive(Deserialize)]
struct WellKnownServer {
    #[serde(rename = "m.server")]
    server: String,
}

/// Returns the location of the matrix server
///
/// This function takes the homeserver entered by the user and finds the address of the matrix server.
pub async fn matrix_server_from_well_known(homeserver: String) -> anyhow::Result<String> {
    let client = reqwest::Client::new();

    let res = client
        .get(format!("https://{homeserver}/.well-known/matrix/server"))
        .send()
        .await
        .context("Failed to get /.well-known/matrix/server")?;

    let status = res.status();

    if !status.is_success() {
        return Err(anyhow!(format!(
            "GET request to /.well-known/matrix/server resulted in an error: {:?}",
            res
        )));
    }

    let server: WellKnownServer = res
        .json()
        .await
        .context("Failed to parse .well-known/matrix/server response")?;

    // TODO: Optionally remove the port if it is 443

    Ok(server.server)
}

#[derive(Debug, Deserialize)]
struct VersionsResponse {
    pub versions: Vec<String>,
}

pub enum MatrixVersion {
    Legacy(String),
    New(String), //TODO: find better name and type for version representation
}

/// Returns the highest supported matrix server version
///
/// Used to ensure that we can call on the server at the v3 endpoint. This function returns an enum between the legacy versions and the newer ones.
pub async fn highest_server_version(server: String) -> anyhow::Result<MatrixVersion> {
    let client = reqwest::Client::new();

    let res = client
        .get(format!("https://{server}/_matrix/client/versions"))
        .send()
        .await
        .context("Failed to get /_matrix/client/versions")?;

    let status = res.status();

    if !status.is_success() {
        return Err(anyhow!(format!(
            "GET request to /.well-known/matrix/server resulted in an error: {:?}",
            res
        )));
    }

    let versions: VersionsResponse = res
        .json()
        .await
        .context("Failed to parse /_matrix/client/versions response")?;

    fn highest_version(prefix: &str, versions: &Vec<String>) -> Option<String> {
        versions
            .iter()
            .filter(|v| v.starts_with(prefix))
            .max_by(|a, b| {
                let va = Version::from(a.strip_prefix(prefix).unwrap_or(a));
                let vb = Version::from(b.strip_prefix(prefix).unwrap_or(b));

                va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Less)
            })
            .cloned()
    }

    if let Some(ver) = highest_version("v", &versions.versions) {
        return Ok(MatrixVersion::New(ver));
    } else if let Some(ver) = highest_version("r", &versions.versions) {
        return Ok(MatrixVersion::Legacy(ver));
    } else {
        return Err(anyhow!(format!(
            "Could not determine highest version from {:?}",
            versions.versions
        )));
    }
}
