use std::sync::{Mutex, RwLock};

use log::trace;
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Default, Clone)]
struct TokenInfo {
    access_token: String,
    refresh_token: String,
    expires_at: u64,
}

impl TokenInfo {
    fn access_header(&self) -> String {
        format!("Bearer {}", self.access_token)
    }

    fn is_stale(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get time")
            .as_secs();

        // Check if it expires within the next 10 seconds
        self.expires_at > now + 30
    }
}

#[derive(Default)]
struct ClientInfo {
    user_id: String,
    device_id: String,
}

#[derive(Default)]
struct AppState {
    token: RwLock<Option<TokenInfo>>,
    client: RwLock<Option<ClientInfo>>,

    matrix_url: RwLock<Option<String>>,

    refresh_lock: Mutex<()>,
}

#[derive(serde::Serialize)]
enum TauriError {
    Wrap(String),
}

impl std::fmt::Debug for TauriError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            TauriError::Wrap(val) => write!(f, "Err({})", val),
        }
    }
}

impl From<anyhow::Error> for TauriError {
    fn from(value: anyhow::Error) -> Self {
        Self::Wrap(value.to_string())
    }
}

impl From<String> for TauriError {
    fn from(value: String) -> Self {
        Self::Wrap(value)
    }
}

impl From<&str> for TauriError {
    fn from(value: &str) -> Self {
        Self::Wrap(value.to_string())
    }
}

impl From<url::ParseError> for TauriError {
    fn from(value: url::ParseError) -> Self {
        Self::Wrap(value.to_string())
    }
}

impl From<()> for TauriError {
    fn from(_value: ()) -> Self {
        Self::Wrap("Unknown error".to_string())
    }
}

#[derive(Serialize)]
struct LoginResponse {
    user_id: String,
}

#[wasm_bindgen]
pub async fn matrix_login(
    matrix_url: String,
    username: String,
    password: String,
    state: AppState,
) -> Result<LoginResponse, TauriError> {
    trace!("Getting login");

    let res = authentication::matrix_login(username, password, matrix_url.clone()).await?;

    let mut client_guard = state.client.write().await;
    let mut token_guard = state.token.write().await;
    let mut url_guard = state.matrix_url.write().await;

    *token_guard = Some(TokenInfo {
        access_token: res.access_token.clone(),
        refresh_token: res.refresh_token.clone(),
        expires_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get time")
            .as_secs()
            + res.expires_in_ms,
    });

    *client_guard = Some(ClientInfo {
        user_id: res.user_id.clone(),
        device_id: res.device_id.clone(),
    });
    *url_guard = Some(matrix_url.clone());

    let rooms = rooms::get_rooms(res.access_token.clone(), matrix_url.clone()).await?;

    println!("{:?}", rooms);

    let sync_res = sync::matrix_sync(res.access_token, matrix_url).await;

    if let Err(e) = sync_res {
        error!("{:?}", e);
        return Err(e);
    } else {
        println!("{:?}", sync_res.unwrap().rooms.join);
    }

    Ok(LoginResponse {
        user_id: res.user_id,
    })
}
