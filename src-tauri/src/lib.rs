use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aes::Aes256;
use aes::cipher::{KeyIvInit, StreamCipher};
use base64::Engine;
use base64::engine::general_purpose;
use colored::Colorize;
use log::info;
use serde::Serialize;
use tauri::async_runtime::{JoinHandle, Mutex, RwLock};
use tauri::{AppHandle, Url};
use tauri::{Manager, State};
use tokio_util::sync::CancellationToken;

use matrix_sdk_crypto::OlmMachine;

mod frontend;
mod matrix_api;
mod storage;

use matrix_api::authentication;
use matrix_api::crypto;
use matrix_api::discovery::choose_home_server;

type Aes256Ctr = ctr::Ctr64BE<Aes256>;

pub const APP_NAME: &str = "opal-matrix";

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use crate::frontend::send_sidebar_update;

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
    app_data_dir: PathBuf,

    token: RwLock<Option<Token>>,
    client: RwLock<Option<ClientInfo>>,

    next_batch: RwLock<Option<String>>,

    matrix_url: RwLock<Option<String>>,

    refresh_lock: Mutex<()>,

    crypto_machine: Mutex<Option<OlmMachine>>,
    recovery_key: RwLock<Option<String>>,

    sync_task: Mutex<Option<JoinHandle<()>>>,
    sync_cancel_token: Mutex<Option<CancellationToken>>,

    connection: Mutex<Option<rusqlite::Connection>>,
    // call_members_by_room: Mutex<HashMap<String, HashSet<String>>>,
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
            expires_at: token.refresh_token.clone().map(|r| r.expires_at),
            next_batch: self.next_batch.read().await.clone(),
            recovery_key: self.recovery_key.read().await.clone(),
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
                    expires_at: session.expires_at.unwrap_or(0),
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

                let mut recovery_guard = self.recovery_key.write().await;
                *recovery_guard = session.recovery_key;

                let mut next_batch_guard = self.next_batch.write().await;
                *next_batch_guard = session.next_batch;
            }

            return Ok(self.check_token().await.is_ok());
        }

        Ok(false)
    }

    async fn check_token(&self) -> Result<String, TauriError> {
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

        let token_guard = self.token.read().await;
        let token = token_guard.as_ref().ok_or("Not logged in")?;

        Ok(token.access_token.clone())
    }

    async fn init_stuff(self: &Arc<Self>, handle: &AppHandle) -> Result<(), TauriError> {
        let client_info = {
            let client_guard = self.client.read().await;
            client_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let db_passphrase = crypto::get_or_create_passphrase(client_info.user_id.clone()).await?;

        let machine = crypto::init_crypto_machine(
            self.app_data_dir.clone(),
            client_info.user_id.clone(),
            client_info.device_id.clone(),
            db_passphrase.clone(),
        )
        .await?;

        let mut machine_guard = self.crypto_machine.lock().await;
        *machine_guard = Some(machine);

        let (already_loaded, conn) = storage::init_storage(
            self.app_data_dir.clone(),
            &client_info.device_id.clone(),
            &db_passphrase,
        )
        .await?;

        {
            let mut conn_guard = self.connection.lock().await;
            *conn_guard = Some(conn);
        }
        self.start_sync(handle, !already_loaded).await?;

        Ok(())
    }

    async fn set_recovery_key(&self, recovery_key: String) -> Result<(), TauriError> {
        {
            let mut key_guard = self.recovery_key.write().await;
            *key_guard = Some(recovery_key.clone());
        }

        let matrix_url = {
            let url_guard = self.matrix_url.read().await;
            url_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let olm_machine = {
            let lock_guard = self.crypto_machine.lock().await;
            lock_guard
                .as_ref()
                .ok_or("Crypto machine not initialized")?
                .clone()
        };

        let token = self.check_token().await?;

        crypto::set_room_keys(&olm_machine, &matrix_url, &token, &recovery_key).await?;

        Ok(())
    }
}

#[derive(serde::Serialize)]
pub enum TauriError {
    Wrap(String),
}

impl TauriError {
    pub fn silent() -> Self {
        Self::Wrap("No error occurred".to_string())
    }
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
async fn try_restore(
    state: State<'_, Arc<AppState>>,
    handle: AppHandle,
) -> Result<Option<LoginResponse>, TauriError> {
    let success = state.login_or_restore_session().await?;

    if success {
        let client_guard = state.client.read().await;

        let client_info = client_guard.as_ref().ok_or("Not logged in")?.clone();

        state.init_stuff(&handle).await?;
        // state.set_recovery_key("".to_string()).await?;

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
    state: State<'_, Arc<AppState>>,
    handle: AppHandle,
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

    state.init_stuff(&handle).await?;

    Ok(LoginResponse {
        user_id: client_info.user_id,
    })
}

#[tauri::command(rename_all = "snake_case")]
async fn set_recovery_key(
    state: State<'_, Arc<AppState>>,
    handle: AppHandle,
    recovery_key: String,
) -> Result<(), TauriError> {
    info!("Setting recovery key");

    state.set_recovery_key(recovery_key).await?;

    state.restart_sync(&handle).await?;

    Ok(())
}

#[tauri::command]
async fn send_frontend(
    state: State<'_, Arc<AppState>>,
    handle: AppHandle,
) -> Result<(), TauriError> {
    let client_guard = state.client.read().await;
    let client_info = client_guard.as_ref().ok_or("Not logged in")?;

    let conn_guard = state.connection.lock().await;
    let conn = conn_guard.as_ref().ok_or("Database not initialized")?;

    send_sidebar_update(conn, &handle, &client_info.user_id.clone())?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app
                .path()
                .data_dir()
                .expect("Failed to get app data dir")
                .join(APP_NAME);

            std::fs::create_dir_all(&data_dir).ok();

            let state = Arc::new(AppState {
                app_data_dir: data_dir,
                ..Default::default()
            });

            app.manage(state);

            Ok(())
        })
        .plugin(
            tauri_plugin_log::Builder::new()
                .level_for("reqwest", log::LevelFilter::Off)
                .level_for("keyring", log::LevelFilter::Off)
                .level_for("matrix_sdk_crypto", log::LevelFilter::Off)
                .level_for("rustls_platform_verifier", log::LevelFilter::Off)
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
                        record.line().unwrap_or(0).to_string().bright_black(),
                        message
                    ));
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            login,
            try_restore,
            set_recovery_key,
            send_frontend,
            choose_home_server,
            storage::get_members,
            matrix_api::rooms::fetch_messages,
            matrix_api::account_data::set_account_data,
            matrix_api::account_data::get_account_data,
            matrix_api::account_data::get_breadcrumbs,
            matrix_api::account_data::get_server_order,
        ])
        .register_asynchronous_uri_scheme_protocol(
            "mxc",
            move |ctx, request: tauri::http::Request<Vec<u8>>, responder| {
                let app_handle = ctx.app_handle().clone();
                let uri = request.uri().to_string();

                tauri::async_runtime::spawn(async move {
                    let state = app_handle.state::<Arc<AppState>>().clone();
                    let Ok(token) = state.check_token().await else {
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(401)
                                .body(Vec::<u8>::new())
                                .unwrap(),
                        );
                        return;
                    };

                    let parsed_url = match Url::parse(&uri) {
                        Ok(u) => u,
                        Err(_) => {
                            responder.respond(
                                tauri::http::Response::builder()
                                    .status(400)
                                    .body(Vec::new())
                                    .unwrap(),
                            );
                            return;
                        }
                    };

                    let server_name = parsed_url.host_str().unwrap_or("");
                    let media_id = parsed_url.path().trim_start_matches('/');

                    let mut is_thumbnail = false;
                    let mut width = "800";
                    let mut height = "800";
                    let mut enc_key = None;
                    let mut enc_iv = None;

                    for (k, v) in parsed_url.query_pairs() {
                        match k.as_ref() {
                            "thumbnail" => is_thumbnail = v == "true",
                            "width" => width = v.into_owned().leak(),
                            "height" => height = v.into_owned().leak(),
                            "key" => enc_key = Some(v.into_owned()),
                            "iv" => enc_iv = Some(v.into_owned()),
                            _ => {}
                        }
                    }

                    let matrix_url = {
                        let url_guard = state.matrix_url.read().await;
                        url_guard
                            .as_ref()
                            .ok_or("Not logged in")
                            .unwrap_or(&"https://matrix-client.matrix.org".to_string())
                            .clone()
                    };

                    let fetch_url = if enc_key.is_some() {
                        format!("{}/_matrix/client/v1/media/download/{}/{}", matrix_url, server_name, media_id)
                    } else if is_thumbnail {
                        format!(
                            "{}/_matrix/client/v1/media/thumbnail/{}/{}?width={}&height={}&method=scale",
                            matrix_url, server_name, media_id, width, height
                        )
                    } else {
                        format!("{}/_matrix/client/v1/media/download/{}/{}", matrix_url, server_name, media_id)
                    };

                    let response = match reqwest::Client::new()
                        .get(&fetch_url)
                        .bearer_auth(token)
                        .send()
                        .await
                    {
                        Ok(res) if res.status().is_success() => {
                            let mut bytes = res.bytes().await.unwrap_or_default().to_vec();

                            if let (Some(k), Some(iv)) = (enc_key, enc_iv) {
                                // Key uses URL_SAFE_NO_PAD, IV uses STANDARD_NO_PAD
                                if let (Ok(key_bytes), Ok(iv_bytes)) = (
                                    general_purpose::URL_SAFE_NO_PAD.decode(&k),
                                    general_purpose::STANDARD_NO_PAD.decode(&iv),
                                ) {
                                    if key_bytes.len() == 32 && iv_bytes.len() >= 8 {
                                        // Pad IV to 16 bytes (Matrix uses 8 bytes of random data + 8 bytes of 0s for counter)
                                        let mut padded_iv = [0u8; 16];
                                        let copy_len = std::cmp::min(iv_bytes.len(), 16);
                                        padded_iv[..copy_len].copy_from_slice(&iv_bytes[..copy_len]);

                                        // Apply AES-256-CTR keystream
                                        if let Ok(mut cipher) = Aes256Ctr::new_from_slices(&key_bytes, &padded_iv) {
                                            cipher.apply_keystream(&mut bytes);
                                        }
                                    } else {
                                        log::error!("Invalid key/iv length for decryption");
                                    }
                                }
                            }

                            tauri::http::Response::builder()
                                .header("Access-Control-Allow-Origin", "*")
                                .body(bytes)
                                .unwrap()
                        }
                        Ok(res) => {
                            log::error!(
                                "Matrix Server rejected media request: HTTP {}",
                                res.status()
                            );
                            tauri::http::Response::builder()
                                .status(res.status().as_u16())
                                .body(Vec::<u8>::new())
                                .unwrap()
                        }
                        Err(e) => {
                            log::error!("Reqwest failed to connect: {}", e);
                            tauri::http::Response::builder()
                                .status(500)
                                .body(Vec::<u8>::new())
                                .unwrap()
                        }
                    };

                    responder.respond(response);
                });
            },
        )
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    info!("Initialized");
}
