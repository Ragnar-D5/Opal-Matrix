#![recursion_limit = "256"]

use log::{error, trace};
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings};
use matrix_sdk::ruma::{OwnedDeviceId, UserId};
use matrix_sdk::{AuthSession, Client as MatrixClient, SessionMeta, SessionTokens};
use ruma::events::room::MediaSource;
use ruma::media::Method;
use shared::api::RestoreResponse;
use shared::api::errors::LoginError;
use std::collections::HashMap;
use std::fs::{read_to_string, write};
use std::sync::Arc;
use tauri::async_runtime::block_on;
use tokio::sync::RwLock;

use aes::Aes256;
use aes::cipher::{KeyIvInit, StreamCipher};
use base64::Engine;
use base64::engine::general_purpose;
use bytes::Bytes;
use chrono::Local;
use log::info;
use tauri::{App, AppHandle, Url, command};
use tauri::{Manager, State};

pub mod frontend;
pub mod matrix_api;
pub mod state;
pub mod storage;
pub(crate) mod sync;

use matrix_api::authentication;

use tauri_plugin_http::reqwest::{self, Response};

type Aes256Ctr = ctr::Ctr64BE<Aes256>;

pub const APP_NAME: &str = "opal-matrix";

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use tauri_plugin_log::{Target, TargetKind};
use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::frontend::send_sidebar_update;
use crate::matrix_api::crypto::{self, StoredSession};
use crate::state::{AppState, TimelineManager};
use crate::sync::attach_callbacks;

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

/// Helper function to convert a reqwest::Response into an http::Response<Bytes>.
///
/// This is necessary because ruma's API expects http::Response types, but reqwest returns its own Response type. This function reads the body of the reqwest response and constructs a new http::Response with the same status, headers, and body content.
async fn reqwest_response_to_http_response(
    res: Response,
) -> Result<http::Response<Bytes>, TauriError> {
    let status = res.status();
    let headers = res.headers().clone();
    let body_bytes = res
        .bytes()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

    let mut http_res_builder = http::Response::builder().status(status);
    for (name, value) in headers.iter() {
        http_res_builder = http_res_builder.header(name, value);
    }

    http_res_builder
        .body(body_bytes)
        .map_err(|e| format!("Failed to build response: {}", e).into())
}

/// Sends a notification with the apphandle if the user has granted permission.
fn send_notification(handle: &AppHandle, title: String, body: String) -> Result<(), TauriError> {
    if handle.notification().permission_state()? != PermissionState::Granted {
        trace!("Notification permission not granted, skipping notification");
        return Ok(());
    }

    let notification = handle.notification().builder().title(title).body(body);

    notification
        .show()
        .map_err(|e| format!("Failed to show notification: {}", e).into())
}

#[derive(serde::Serialize)]
pub enum TauriError {
    Wrap(String),
}

pub trait AsInfo {
    fn as_info(self) -> TauriError;
}

impl<T> AsInfo for T
where
    T: ToString,
{
    fn as_info(self) -> TauriError {
        TauriError::as_info(self.to_string())
    }
}

impl TauriError {
    pub fn silent() -> Self {
        Self::Wrap("No error occurred".to_string())
    }

    pub fn as_info<T: ToString>(value: T) -> Self {
        let val = value.to_string();
        let location = std::panic::Location::caller();

        log::logger().log(
            &log::Record::builder()
                .args(format_args!("{}", val))
                .level(log::Level::Info)
                .target(module_path!())
                .file(Some(location.file()))
                .line(Some(location.line()))
                .build(),
        );
        Self::Wrap(val)
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

#[command]
async fn try_restore(
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    matrix_client: State<'_, RwLock<MatrixClient>>,
) -> Result<RestoreResponse, TauriError> {
    let Some(session) = state.get_last_session().await? else {
        return Ok(RestoreResponse::NoSession);
    };

    let path = app_handle
        .path()
        .app_data_dir()?
        .join("sessions")
        .join(format!("{}-{}.db", session.user_id, session.device_id));

    let new_client = MatrixClient::builder()
        .homeserver_url(session.homeserver_url.clone())
        .handle_refresh_tokens()
        .sqlite_store(path, None)
        .build()
        .await?;

    let Some(user_id) = state.login_or_restore_session(session.clone()).await? else {
        return Ok(RestoreResponse::Failed {
            home_server: session.homeserver_url,
        });
    };

    let session = AuthSession::Matrix(MatrixSession {
        meta: SessionMeta {
            device_id: OwnedDeviceId::from(session.device_id.to_string()),
            user_id: UserId::parse(session.user_id)?,
        },
        tokens: SessionTokens {
            access_token: session.access_token,
            refresh_token: session.refresh_token,
        },
    });
    new_client.restore_session(session).await?;
    attach_callbacks(&new_client, &app_handle).await?;

    *matrix_client.write().await = new_client;

    // state.init_stuff(&handle).await?;

    Ok(RestoreResponse::Success { user_id })
}

#[tauri::command(rename_all = "snake_case")]
async fn login(
    username: String,
    password: String,
    recovery_key: String,
    state: State<'_, Arc<AppState>>,
    app_handle: AppHandle,
    matrix_client: State<'_, RwLock<MatrixClient>>,
    handle: AppHandle,
) -> Result<String, LoginError> {
    info!("Logging in new");

    state.stop_sync().await.map_err(|e| {
        error!("Failed to stop sync: {:?}", e);
        LoginError::BackendError
    })?;

    let server_url = matrix_client.read().await.homeserver().to_string();

    let temp_client = MatrixClient::builder()
        .homeserver_url(
            Url::parse(server_url.as_str()).expect("Valid homeserverurl from other client"),
        )
        .build()
        .await
        .map_err(|e| {
            error!("Failed to build temporary client: {:?}", e);
            LoginError::BackendError
        })?;

    temp_client
        .matrix_auth()
        .login_username(username.clone(), password.as_str())
        .initial_device_display_name("Opal on Linux")
        .send()
        .await
        .map_err(|e| {
            error!("Login failed: {:?}", e);
            LoginError::InvalidCredentials
        })?;

    let user_id = temp_client.user_id().unwrap();
    let device_id = temp_client.device_id().unwrap();

    let path = app_handle
        .path()
        .app_data_dir()
        .expect("Failed to get app data dir")
        .join("sessions")
        .join(format!("{}-{}.db", user_id, device_id));

    let new_client = MatrixClient::builder()
        .homeserver_url(
            Url::parse(server_url.as_str()).expect("Valid homeserverurl from other client"),
        )
        .handle_refresh_tokens()
        .sqlite_store(path, None)
        .build()
        .await
        .map_err(|e| {
            error!("Failed to build new client: {:?}", e);
            LoginError::BackendError
        })?;

    new_client
        .restore_session(temp_client.session().unwrap().clone())
        .await
        .map_err(|e| {
            error!("Failed to restore session on new client: {:?}", e);
            LoginError::BackendError
        })?;

    new_client
        .encryption()
        .recovery()
        .recover(recovery_key.as_str())
        .await
        .map_err(|e| {
            error!("Recovery failed: {:?}", e);
            LoginError::InvalidCredentials
        })?;

    // new_client.encryption().backups()

    new_client
        .encryption()
        .get_device(user_id, device_id)
        .await
        .expect("Failed to get device info after recovery")
        .unwrap()
        .verify()
        .await
        .map_err(|e| {
            error!("Failed to verify device: {e}");
            LoginError::InvalidCredentials
        })?;

    attach_callbacks(&new_client, &handle).await.map_err(|e| {
        error!("Failed to start sync loop: {:?}", e);
        LoginError::BackendError
    })?;

    let user_id = new_client.user_id().unwrap().to_string();
    *matrix_client.write().await = new_client;

    let session = StoredSession {
        user_id: user_id.clone(),
        device_id: device_id.to_string(),
        access_token: temp_client.session().unwrap().access_token().to_string(),
        refresh_token: temp_client
            .session()
            .unwrap()
            .get_refresh_token()
            .map(|t| t.to_string()),
        homeserver_url: server_url,
        expires_at: None,
        next_batch: None,
        recovery_key: Some(recovery_key),
    };

    crypto::save_session(&session).await.map_err(|e| {
        error!("Failed to save session: {:?}", e);
        LoginError::BackendError
    })?;

    // let server_info = state
    //     .home_server_info
    //     .read()
    //     .await
    //     .as_ref()
    //     .ok_or_else(|| {
    //         error!("No server info");
    //         LoginError::BackendError
    //     })?
    //     .clone();

    // let (client_info, token) = matrix_login(server_info, username, password).await?;

    // {
    //     let mut client_guard = state.client.write().await;
    //     let mut token_guard = state.token.write().await;

    //     *token_guard = Some(token.clone());
    //     *client_guard = Some(client_info.clone());
    // }

    // state.save_session().await.map_err(|e| {
    //     error!("Failed to save session: {:?}", e);
    //     LoginError::BackendError
    // })?;

    // state.init_stuff(&handle).await.map_err(|e| {
    //     error!("Failed to initialize stuff: {:?}", e);
    //     LoginError::BackendError
    // })?;

    // state.set_recovery_key(recovery_key).await.map_err(|e| {
    //     error!("Failed to set recovery key: {:?}", e);
    //     LoginError::BackendError
    // })?;

    Ok(user_id)
}

/// Sets the recovery key for the current user. The key is saved in the keyring.
#[command(rename_all = "snake_case")]
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

#[command]
async fn fetch_raw_html(url: String) -> Result<String, TauriError> {
    let res = reqwest::get(url).await?;

    res.text().await.map_err(|e| e.into())
}

#[command(rename_all = "snake_case")]
async fn set_room_id(
    state: State<'_, Arc<AppState>>,
    room_id: Option<String>,
) -> Result<(), TauriError> {
    let mut room_id_guard = state.frontend_current_room_id.write().await;
    *room_id_guard = room_id;

    Ok(())
}

#[command(rename_all = "snake_case")]
async fn set_frontend_focused(
    state: State<'_, Arc<AppState>>,
    focused: bool,
) -> Result<(), TauriError> {
    let mut focused_guard = state.frontend_is_focused.write().await;
    *focused_guard = focused;

    Ok(())
}

#[command(rename_all = "snake_case")]
async fn backend_log(
    level: String,
    timestamp: String,
    path: String,
    line: Option<u32>,
    message: String,
) {
    let level = match level.as_str() {
        "ERROR" => log::Level::Error,
        "WARN" => log::Level::Warn,
        "INFO" => log::Level::Info,
        "DEBUG" => log::Level::Debug,
        "TRACE" => log::Level::Trace,
        _ => log::Level::Info,
    };

    let combined_message = format!("[FE Time: {}] {}", timestamp, message);

    log::logger().log(
        &log::Record::builder()
            .args(format_args!("{}", combined_message))
            .level(level)
            .target(module_path!())
            .file(Some(path.as_str()))
            .line(line)
            .build(),
    );
}

pub struct BrandColorsMap(pub HashMap<String, String>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .setup(|app: &mut App| {
            let config_dir = app.path().app_config_dir().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to resolve app config dir: {e}"),
                )
            })?;

            std::fs::create_dir_all(&config_dir)?;

            let brand_colors_file_path = config_dir.join("brand_colors.json");

            let color_map: HashMap<String, String> = if brand_colors_file_path.exists() {
                let json_str = read_to_string(&brand_colors_file_path).unwrap_or_default();
                serde_json::from_str(&json_str).unwrap_or_default()
            } else {
                let default_json = serde_json::json!({
                    "youtube.com": "#FF0000",
                    "youtu.be": "#FF0000",
                    "reddit.com": "#FF4500",
                    "twitter.com": "#1DA1F2",
                    "github.com": "#24292e",
                    "spotify.com": "#1DB954",
                    "netflix.com": "#E50914",
                    "codeberg.org": "#f05133",
                });

                let pretty_json = serde_json::to_string_pretty(&default_json).unwrap();
                write(&brand_colors_file_path, pretty_json)
                    .expect("Failed to write default brands.json");

                serde_json::from_value(default_json).unwrap()
            };

            app.manage(BrandColorsMap(color_map));

            let data_dir = app.path().app_data_dir().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to resolve app data dir: {e}"),
                )
            })?;

            std::fs::create_dir_all(&data_dir)?;

            let log_dir = data_dir
                .join("logs")
                .join(Local::now().format("%Y-%m-%d").to_string());
            std::fs::create_dir_all(&log_dir)?;
            let log_file = format!("{}.log", Local::now().format("%H-%M-%S"));

            app.handle().plugin(
                tauri_plugin_log::Builder::new()
                    .level(log::LevelFilter::Debug)
                    .targets([
                        Target::new(tauri_plugin_log::TargetKind::Stdout),
                        Target::new(TargetKind::Folder {
                            path: log_dir,
                            file_name: Some(log_file),
                        }),
                    ])
                    .level_for("reqwest", log::LevelFilter::Off)
                    .level_for("keyring", log::LevelFilter::Off)
                    .level_for("matrix_sdk_crypto", log::LevelFilter::Off)
                    .level_for("rustls_platform_verifier", log::LevelFilter::Off)
                    .level_for("html5ever", log::LevelFilter::Off)
                    .format(|out, message, record| {
                        let level = match record.level() {
                            log::Level::Error => "ERROR",
                            log::Level::Warn => "WARN",
                            log::Level::Info => "INFO",
                            log::Level::Debug => "DEBUG",
                            log::Level::Trace => "TRACE",
                        };

                        let time = chrono::offset::Local::now()
                            .format("%Y-%m-%d %H:%M:%S.%3f")
                            .to_string();

                        out.finish(format_args!(
                            "{}|{}|{}:{}|{}",
                            level,
                            time,
                            record.file().unwrap_or("Unknown"),
                            record.line().unwrap_or(0),
                            message
                        ));
                    })
                    .build(),
            )?;

            let state = Arc::new(AppState {
                app_data_dir: data_dir,
                ..Default::default()
            });

            app.manage(state);

            let client = block_on(async {
                MatrixClient::builder()
                    .handle_refresh_tokens()
                    .homeserver_url("https://matrix.org")
                    .build()
                    .await
                    .unwrap()
            });

            app.manage(RwLock::new(client));
            app.manage(TimelineManager::default());

            #[cfg(not(target_os = "android"))]
            let main_window = app
                .get_webview_window("main")
                .expect("Failed to get main window");

            #[cfg(not(target_os = "android"))]
            main_window.maximize().ok();

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            login,
            fetch_raw_html,
            try_restore,
            set_recovery_key,
            send_frontend,
            backend_log,
            set_room_id,
            set_frontend_focused,
            // frontend commands
            frontend::messages::commit_message,
            frontend::messages::get_timeline,
            frontend::messages::scroll_up,
            frontend::commands::get_commands,
            // storage commands
            storage::get_members,
            storage::members::get_members_for_room,
            storage::receipts::get_receipt,
            // matrix API commands
            matrix_api::discovery::choose_home_server,
            // matrix_api::messages::fetch_messages,
            matrix_api::rooms::send_read_marker,
            matrix_api::account_data::set_account_data,
            matrix_api::account_data::get_account_data,
            matrix_api::account_data::get_breadcrumbs,
            matrix_api::account_data::get_server_order,
            matrix_api::previews::get_url_preview,
        ])
        .register_asynchronous_uri_scheme_protocol(
            "mxc",
            move |ctx, request: tauri::http::Request<Vec<u8>>, responder| {
                let app_handle = ctx.app_handle().clone();
                let uri = request.uri().to_string();

                tauri::async_runtime::spawn(async move {
                    // 1. Parse the requested URI [cite: 221]
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

                    // 2. Reconstruct to a pure MXC string and parse into Ruma's OwnedMxcUri
                    let mxc_string = format!("mxc://{}/{}", server_name, media_id);
                    let Ok(owned_mxc_uri): Result<ruma::OwnedMxcUri, std::convert::Infallible> =
                        matrix_sdk::ruma::OwnedMxcUri::try_from(mxc_string)
                    else {
                        responder.respond(
                            tauri::http::Response::builder()
                                .status(400)
                                .body(Vec::new())
                                .unwrap(),
                        );
                        return;
                    };

                    let mut is_thumbnail = false;
                    let mut width: u32 = 800;
                    let mut height: u32 = 800;
                    let mut enc_key = None;
                    let mut enc_iv = None;

                    // Parse frontend parameters
                    for (k, v) in parsed_url.query_pairs() {
                        match k.as_ref() {
                            "thumbnail" => is_thumbnail = v == "true",
                            "width" => width = v.parse().unwrap_or(800),
                            "height" => height = v.parse().unwrap_or(800),
                            "key" => enc_key = Some(v.into_owned()),
                            "iv" => enc_iv = Some(v.into_owned()),
                            _ => {}
                        }
                    }

                    // 3. Retrieve MatrixClient directly from Tauri state [cite: 128, 212]
                    let client = app_handle
                        .state::<tokio::sync::RwLock<matrix_sdk::Client>>()
                        .read()
                        .await
                        .clone();

                    // 4. Build the MediaRequest
                    let source = MediaSource::Plain(owned_mxc_uri);
                    let format = if is_thumbnail {
                        MediaFormat::Thumbnail(MediaThumbnailSettings {
                            method: Method::Scale,
                            width: width.into(),
                            height: height.into(),
                            animated: false,
                        })
                    } else {
                        MediaFormat::File
                    };

                    let media_request = MediaRequestParameters { source, format };

                    // 5. Fetch using the Matrix SDK (handles tokens and caching natively)
                    match client.media().get_media_content(&media_request, true).await {
                        Ok(mut bytes) => {
                            // Custom AES-256-CTR Decryption logic [cite: 237, 238, 241, 242]
                            if let (Some(k), Some(iv)) = (enc_key, enc_iv)
                                && let (Ok(key_bytes), Ok(iv_bytes)) = (
                                    general_purpose::URL_SAFE_NO_PAD.decode(&k),
                                    general_purpose::STANDARD_NO_PAD.decode(&iv),
                                )
                            {
                                if key_bytes.len() == 32 && iv_bytes.len() >= 8 {
                                    let mut padded_iv = [0u8; 16];
                                    let copy_len = std::cmp::min(iv_bytes.len(), 16);
                                    padded_iv[..copy_len].copy_from_slice(&iv_bytes[..copy_len]);

                                    if let Ok(mut cipher) =
                                        Aes256Ctr::new_from_slices(&key_bytes, &padded_iv)
                                    {
                                        cipher.apply_keystream(&mut bytes);
                                    }
                                } else {
                                    log::error!("Invalid key/iv length for decryption");
                                }
                            }

                            responder.respond(
                                tauri::http::Response::builder()
                                    .header("Access-Control-Allow-Origin", "*")
                                    .body(bytes)
                                    .unwrap(),
                            );
                        }
                        Err(e) => {
                            log::error!("Matrix SDK rejected media request: {}", e);
                            responder.respond(
                                tauri::http::Response::builder()
                                    .status(500)
                                    .body(Vec::<u8>::new())
                                    .unwrap(),
                            );
                        }
                    }
                });
            },
        )
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    info!("Initialized");
}
