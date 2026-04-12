// TODO: remove when stable
#![allow(dead_code, unused_imports)]

use std::time::{SystemTime, UNIX_EPOCH};

use colored::Colorize;
use log::{debug, error, info, trace};
use ruma::api::client::state;
use serde::Serialize;
use tauri::async_runtime::{Mutex, RwLock};
use tauri::Url;
use tauri::{Manager, State};

use matrix_sdk_crypto::OlmMachine;
use matrix_sdk_sqlite::SqliteCryptoStore;

mod matrix_api;
use matrix_api::authentication;
use matrix_api::crypto;
use matrix_api::rooms;
use matrix_api::sync;

pub const APP_NAME: &str = "Maru";

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};

use crate::matrix_api::authentication::refresh_token;

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
struct RefreshToken {
    token: String,
    expires_at: u64,
}

#[derive(Default, Clone)]
struct Token {
    access_token: String,

    refresh_token: Option<RefreshToken>,
}

impl Token {
    fn access_header(&self) -> String {
        format!("Bearer {}", self.access_token)
    }

    fn is_stale(&self) -> bool {
        let expires_at = if let Some(refresh) = &self.refresh_token {
            refresh.expires_at
        } else {
            return false;
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get time")
            .as_secs();

        now + 30 >= expires_at
    }
}

#[derive(Default, Clone)]
struct ClientInfo {
    user_id: String,
    device_id: String,
}

#[derive(Default)]
struct AppState {
    token: RwLock<Option<Token>>,
    client: RwLock<Option<ClientInfo>>,

    matrix_url: RwLock<Option<String>>,

    refresh_lock: Mutex<()>,

    crypto_machine: Mutex<Option<OlmMachine>>,
}

impl AppState {
    async fn save_session(&self) -> Result<(), TauriError> {
        let token = {
            let token_guard = self.token.read().await;
            token_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let client_info = {
            let client_guard = self.client.read().await;
            client_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let matrix_url = {
            let url_guard = self.matrix_url.read().await;
            url_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let session = crypto::StoredSession {
            user_id: client_info.user_id.clone(),
            device_id: client_info.device_id.clone(),
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token.clone().map(|r| r.token),
            homeserver_url: matrix_url.clone(),
        };

        crypto::save_session(&session).await?;

        Ok(())
    }

    async fn refresh_token(&self) -> Result<(), TauriError> {
        let refresh_token = {
            let token_guard = self.token.read().await;
            let token = token_guard.as_ref().ok_or("Not logged in")?.clone();

            if let Some(refresh) = token.refresh_token {
                refresh
            } else {
                return Err("No refresh token available".into());
            }
        };

        let matrix_url = {
            let url_guard = self.matrix_url.read().await;
            url_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let res = authentication::refresh_token(refresh_token.token, matrix_url).await?;

        {
            let mut write_guard = self.token.write().await;
            *write_guard = Some(res);
        }

        self.save_session().await?;

        return Ok(());
    }

    async fn login_or_restore_session(&self) -> Result<bool, TauriError> {
        let session = crypto::get_last_active_session().await?;

        if let Some(session) = session {
            let token = Token {
                access_token: session.access_token,
                refresh_token: session.refresh_token.map(|r| RefreshToken {
                    token: r,
                    expires_at: 0,
                }),
            };

            let client_info = ClientInfo {
                user_id: session.user_id.clone(),
                device_id: session.device_id.clone(),
            };

            {
                let mut token_guard = self.token.write().await;
                *token_guard = Some(token);

                let mut client_guard = self.client.write().await;
                *client_guard = Some(client_info);

                let mut url_guard = self.matrix_url.write().await;
                *url_guard = Some(session.homeserver_url);
            }

            return Ok(self.refresh_token().await.is_ok());
        }
        Ok(false)
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

    async fn init_stuff(&self) -> Result<(), TauriError> {
        let client_info = {
            let client_guard = self.client.read().await;
            client_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let db_passphrase = crypto::get_or_create_passphrase(client_info.user_id.clone()).await?;

        let machine = crypto::init_crypto_machine(
            client_info.user_id.clone(),
            client_info.device_id,
            db_passphrase,
        )
        .await?;

        let mut machine_guard = self.crypto_machine.lock().await;
        *machine_guard = Some(machine);

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

impl<T> From<T> for TauriError
where
    T: ToString,
{
    #[track_caller]
    fn from(value: T) -> Self {
        let val = value.to_string();
        let location = std::panic::Location::caller();

        log::logger().log(
            &log::Record::builder()
                .args(format_args!("{}", val))
                .level(log::Level::Error)
                .target(module_path!())
                .file(Some(location.file()))
                .line(Some(location.line()))
                .build(),
        );

        Self::Wrap(val)
    }
}

#[derive(Serialize)]
struct LoginResponse {
    user_id: String,
}

#[tauri::command]
async fn try_restore(state: State<'_, AppState>) -> Result<Option<LoginResponse>, TauriError> {
    let success = state.login_or_restore_session().await?;

    if success {
        let client_guard = state.client.read().await;

        let client_info = client_guard.as_ref().ok_or("Not logged in")?.clone();

        state.init_stuff().await?;

        Ok(Some(LoginResponse {
            user_id: client_info.user_id,
        }))
    } else {
        Ok(None)
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn login(
    matrix_url: String,
    username: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<LoginResponse, TauriError> {
    info!("Logging in new");
    let (client_info, token) =
        authentication::matrix_login(username, password, matrix_url.clone()).await?;
    {
        let mut client_guard = state.client.write().await;
        let mut token_guard = state.token.write().await;
        let mut url_guard = state.matrix_url.write().await;

        *token_guard = Some(token.clone());
        *client_guard = Some(client_info.clone());
        *url_guard = Some(matrix_url.clone());
    }

    state.save_session().await?;

    state.init_stuff().await?;

    Ok(LoginResponse {
        user_id: client_info.user_id,
    })
}

#[tauri::command]
async fn first_sync(state: State<'_, AppState>) -> Result<(), TauriError> {
    info!("Starting first sync");

    let lock_guard = state.crypto_machine.lock().await;
    let olm_machine = lock_guard
        .as_ref()
        .ok_or("Crypto machine not initialized")?;

    let token = {
        let token_guard = state.token.read().await;
        token_guard.as_ref().ok_or("Not logged in")?.clone()
    };

    let matrix_url = {
        let url_guard = state.matrix_url.read().await;
        url_guard.as_ref().ok_or("Not logged in")?.clone()
    };

    let sync_res = sync::matrix_sync(&token.access_token, &matrix_url).await?;

    let res =
        crypto::process_sync_response(olm_machine, sync_res, &token.access_token, &matrix_url)
            .await?;

    println!("Processed sync response: {:#?}", res);

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::default();

    info!("Initialized");

    tauri::Builder::default()
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

                    out.finish(format_args!(
                        "{}|{}|{}|{}|{}",
                        level,
                        time,
                        record.file().unwrap_or("Unknown").cyan(),
                        record.line().unwrap_or(0).to_string().black(),
                        message
                    ));
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![login, first_sync, try_restore])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
