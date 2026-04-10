use std::time::{SystemTime, UNIX_EPOCH};

use crate::{TauriError, TokenInfo};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::utils::platform::Target;

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

#[derive(Deserialize)]
pub struct MatrixLoginResponse {
    pub access_token: String,
    pub refresh_token: String,

    pub device_id: String,
    pub user_id: String,

    pub expires_in_ms: u64,
}

impl Into<TokenInfo> for MatrixRefreshResponse {
    fn into(self) -> TokenInfo {
        TokenInfo {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Failed to get time")
                .as_secs()
                + self.expires_in_ms,
        }
    }
}

pub async fn matrix_login(
    username: String,
    password: String,
    matrix_url: String,
) -> Result<MatrixLoginResponse, TauriError> {
    let client = Client::new();

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
        .map_err(|e| format!("Network error: {}", e))
        .unwrap();

    if res.status().is_success() {
        let json_res: MatrixLoginResponse = res
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;

        return Ok(json_res);
    } else {
        return Err(format!("Web request failed: {}", res.status()).into());
    }
}

#[derive(Serialize)]
struct MatrixRefreshRequest {
    refresh_token: String,
}

#[derive(Serialize, Deserialize)]
struct MatrixRefreshResponse {
    access_token: String,
    refresh_token: String,

    expires_in_ms: u64,
}

pub async fn refresh_token(
    refresh_token: String,
    matrix_url: String,
) -> Result<TokenInfo, TauriError> {
    let client = Client::new();
    let payload = MatrixRefreshRequest {
        refresh_token: refresh_token,
    };

    let res = client
        .post(format!("{matrix_url}/_matrix/client/v3/refresh"))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if res.status().is_success() {
        let json_res: MatrixRefreshResponse =
            res.json().await.map_err(|e| format!("Parse error: {e}"))?;

        return Ok(json_res.into());
    } else {
        return Err(format!("Web request failed: {}", res.status()).into());
    }
}
