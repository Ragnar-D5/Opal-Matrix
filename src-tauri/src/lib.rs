use std::time::{SystemTime, UNIX_EPOCH};

use colored::Colorize;
use log::{error, info, trace};
use serde::Serialize;
use tauri::async_runtime::{Mutex, RwLock};
use tauri::Url;
use tauri::{Manager, State};

mod matrix_api;
use matrix_api::authentication;
use matrix_api::rooms;
use matrix_api::sync;

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

const MATRIX_ID_SET: &AsciiSet = &CONTROLS.add(b'!').add(b':');

fn construct_url<S>(parts: Vec<S>) -> Result<Url, TauriError>
where
    S: AsRef<str>,
{
    let mut iter = parts.into_iter();

    let first = iter.next().ok_or_else(|| "Empty path".to_string())?;
    let mut url_str = first.as_ref().trim_end_matches('/').to_string();

    for part in iter {
        let encoded = utf8_percent_encode(part.as_ref(), MATRIX_ID_SET).to_string();

        url_str.push('/');
        url_str.push_str(&encoded);
    }

    return Url::parse(&url_str).map_err(|e| format!("Invalid URL: {}", e).into());
}

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

impl AppState {
    async fn refresh_token(&self) -> Result<(), TauriError> {
        let token = {
            let token_guard = self.token.read().await;
            token_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let matrix_url = {
            let url_guard = self.matrix_url.read().await;
            url_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let res = authentication::refresh_token(token.refresh_token, matrix_url).await?;

        let mut write_guard = self.token.write().await;
        *write_guard = Some(res);

        return Ok(());
    }
    async fn check_token(&self) -> Result<(), TauriError> {
        let needs_refresh = {
            let token_guard = self.token.read().await;
            let t = token_guard.as_ref().ok_or("Not logged in")?;
            t.is_stale()
        };

        if needs_refresh {
            let _lock = self.refresh_lock.lock().await;

            // Check if another thread already refreshed it
            let still_needs_refresh = {
                let token_guard = self.token.read().await;
                token_guard.as_ref().map(|t| t.is_stale()).unwrap_or(false)
            };

            if still_needs_refresh {
                self.refresh_token().await?;
            }
        }

        Ok(())
    }
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

#[tauri::command(rename_all = "snake_case")]
async fn matrix_login(
    matrix_url: String,
    username: String,
    password: String,
    state: State<'_, AppState>,
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
