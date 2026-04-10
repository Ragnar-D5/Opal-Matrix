use std::ops::Deref;

use colored::Colorize;
use log::{error, info, trace};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::async_runtime::{Mutex, RwLock};
use tauri::{Manager, State};

mod authentication;
use authentication::login;

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
struct MatrixLoginResponse {
    user_id: String,
    device_id: String,

    access_token: String,
    refresh_token: String,
    expires_in_ms: u64,
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

#[derive(Default, Clone)]
struct TokenInfo {
    access_token: String,
    refresh_token: String,
    expires_in_ms: u64,
}

impl TokenInfo {
    fn access_header(self: &TokenInfo) -> String {
        format!("Bearer {}", self.access_token)
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
}

#[derive(serde::Serialize)]
enum TauriError {
    Wrap(String),
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

impl<T> From<Result<T, String>> for TauriError {
    fn from(value: Result<T, String>) -> Self {
        Self::Wrap(value.err().unwrap_or("Unknown error".to_string()))
    }
}

async fn refresh_token(state: &AppState) -> Result<(), TauriError> {
    let client = Client::new();

    let (url, token) = {
        let url_guard = state.matrix_url.read().await;

        let token_guard = state.token.read().await;

        let url = url_guard.clone().ok_or("No matrix url saved")?;
        let token = token_guard.clone().ok_or("No token saved")?;

        (url, token)
    };

    let payload = MatrixRefreshRequest {
        refresh_token: token.refresh_token,
    };

    let res = client
        .post(format!("{url}/_matrix/client/v3/refresh"))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if res.status().is_success() {
        let json_res: MatrixRefreshResponse = res
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;

        let mut write_guard = state.token.write().await;
        *write_guard = Some(TokenInfo {
            access_token: json_res.access_token,
            refresh_token: json_res.refresh_token,
            expires_in_ms: json_res.expires_in_ms,
        });
    } else {
        return Err(format!("Failed to refresh: {}", res.status()).into());
    }

    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
async fn matrix_login(
    matrix_url: String,
    username: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), TauriError> {
    trace!("Getting login");

    let res = login::matrix_login(username, password, matrix_url.clone()).await?;

    let mut client_guard = state.client.write().await;
    let mut token_guard = state.token.write().await;
    let mut url_guard = state.matrix_url.write().await;

    *token_guard = Some(TokenInfo {
        access_token: res.access_token.clone(),
        refresh_token: res.refresh_token.clone(),
        expires_in_ms: res.expires_in_ms,
    });
    *client_guard = Some(ClientInfo {
        user_id: res.user_id.clone(),
        device_id: res.device_id.clone(),
    });
    *url_guard = Some(matrix_url);

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::default();

    info!("Initialized");

    tauri::Builder::default()
        .setup(|app| {
            app.manage(Mutex::new(AppState::default()));
            Ok(())
        })
        .manage(state)
        .plugin(
            tauri_plugin_log::Builder::new()
                .format(|out, message, record| {
                    let level = match record.level() {
                        log::Level::Error => "ERROR".red().bold(),
                        log::Level::Warn => "WARN".yellow().bold(),
                        log::Level::Info => "INFO".green().bold(),
                        log::Level::Debug => "DEBUG".blue().bold(),
                        log::Level::Trace => "TRACE".magenta().bold(),
                    };

                    let time = chrono::offset::Local::now()
                        .format("%d.%m.%Y %H:%M.%3f")
                        .to_string()
                        .bright_black()
                        .italic();

                    out.finish(format_args!("{}|{}|{}", level, time, message));
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![matrix_login])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
