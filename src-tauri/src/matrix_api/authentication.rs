use std::time::{SystemTime, UNIX_EPOCH};

use crate::{construct_url, AppState, ClientInfo};
use log::debug;
use serde_json::Value;

use crate::{RefreshToken, TauriError, Token};
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

#[derive(Deserialize)]
pub struct MatrixLoginResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,

    pub device_id: String,
    pub user_id: String,

    pub expires_in_ms: Option<u64>,
}

impl Into<(ClientInfo, Token)> for MatrixLoginResponse {
    fn into(self) -> (ClientInfo, Token) {
        let refresh_token =
            if let (Some(token), Some(ms)) = (self.refresh_token, self.expires_in_ms) {
                Some(RefreshToken {
                    token: token,
                    expires_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("Failed to get time")
                        .as_secs()
                        + ms / 1000,
                })
            } else {
                None
            };

        return (
            ClientInfo {
                user_id: self.user_id,
                device_id: self.device_id,
            },
            Token {
                access_token: self.access_token,
                refresh_token: refresh_token,
            },
        );
    }
}

pub async fn matrix_login(
    username: String,
    password: String,
    matrix_url: String,
) -> Result<(ClientInfo, Token), TauriError> {
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
        .map_err(|e| format!("Network error: {}", e))?;

    if res.status().is_success() {
        let json_res: MatrixLoginResponse = res
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;

        return Ok(json_res.into());
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
    refresh_token: Option<String>,

    expires_in_ms: Option<u64>,
}

impl Into<Token> for MatrixRefreshResponse {
    fn into(self) -> Token {
        let refresh_token =
            if let (Some(token), Some(ms)) = (self.refresh_token, self.expires_in_ms) {
                Some(RefreshToken {
                    token: token,
                    expires_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("Failed to get time")
                        .as_secs()
                        + ms / 1000,
                })
            } else {
                None
            };
        return Token {
            access_token: self.access_token,
            refresh_token: refresh_token,
        };
    }
}

pub async fn refresh_token(refresh_token: String, matrix_url: String) -> Result<Token, TauriError> {
    let client = Client::new();
    let payload = MatrixRefreshRequest {
        refresh_token: refresh_token,
    };

    let url = construct_url(vec![
        matrix_url,
        "_matrix".to_string(),
        "client".to_string(),
        "v3".to_string(),
        "refresh".to_string(),
    ])?;

    let res = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if res.status().is_success() {
        let json_res: MatrixRefreshResponse =
            res.json().await.map_err(|e| format!("Parse error: {e}"))?;

        return Ok(json_res.into());
    } else {
        return Err(format!(
            "Web request failed: {}, {}",
            res.status(),
            res.text().await.unwrap_or_else(|_| "Unknown error".into())
        )
        .into());
    }
}

pub async fn get_account_data(
    token: &String,
    matrix_url: &String,
    user_id: &String,
    data_type: &String,
) -> Result<Value, TauriError> {
    let client = Client::new();

    let url = construct_url(vec![
        matrix_url,
        "_matrix",
        "client",
        "v3",
        "user",
        user_id,
        "account_data",
        data_type,
    ])?;

    let res = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if res.status().is_success() {
        let json_res: Value = res.json().await.map_err(|e| format!("Parse error: {e}"))?;

        return Ok(json_res);
    } else {
        return Err(format!("Web request failed: {}", res.status()).into());
    }
}
