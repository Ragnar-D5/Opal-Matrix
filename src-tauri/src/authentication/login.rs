use crate::{AppState, TauriError};
use log::{error, info, trace};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct MatrixLoginIdentifier {
    #[serde(rename = "type")]
    id_type: String,
    user: String,
}

#[derive(Serialize)]
struct MatrixLoginRequest {
    #[serde(rename = "type")]
    login_type: String,
    identifier: MatrixLoginIdentifier,
    password: String,
    refresh_token: bool,
}

#[derive(Serialize, Deserialize)]
pub struct MatrixLoginResponse {
    pub user_id: String,
    pub device_id: String,

    pub access_token: String,
    pub refresh_token: String,
    pub expires_in_ms: u64,
}

#[derive(Serialize)]
struct MatrixRefreshRequest {
    refresh_token: String,
}

#[derive(Deserialize)]
struct MatrixRefreshResponse {
    access_token: String,
    refresh_token: String,

    expires_in_ms: u64,
}

pub async fn matrix_login(
    username: String,
    password: String,
    matrix_url: String,
) -> Result<MatrixLoginResponse, TauriError> {
    let client = Client::new();

    trace!("Getting login");

    let payload = MatrixLoginRequest {
        login_type: "m.login.password".to_string(),
        identifier: MatrixLoginIdentifier {
            id_type: "m.id.user".to_string(),
            user: username,
        },
        password: password,
        refresh_token: true,
    };

    let res = client
        .post(format!("{matrix_url}/_matrix/client/v3/login"))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if res.status().is_success() {
        let json_res: MatrixLoginResponse = res
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;

        info!("Successfully logged in as {}", json_res.user_id);

        return Ok(json_res);
    } else {
        error!("Failed to log in: {}", res.status());

        return Err("Failed to log in".into());
    }
}
