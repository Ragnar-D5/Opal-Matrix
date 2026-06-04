#![recursion_limit = "256"]

use const_format::formatcp;
use log::{error, trace};
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::ruma::{OwnedDeviceId, UserId};
use matrix_sdk::{AuthSession, Client as MatrixClient, SessionMeta, SessionTokens};
use percent_encoding::percent_decode_str;
use shared::api::RestoreResponse;
use shared::api::errors::LoginError;
use std::collections::HashMap;
use std::fs::{read_to_string, write};
use std::sync::Arc;
use tauri::async_runtime::block_on;
use tokio::sync::RwLock;

use bytes::Bytes;
use chrono::Local;
use log::info;
use tauri::{App, AppHandle, Url, command};
use tauri::{Manager, State};

pub mod frontend;
pub mod matrix_api;
pub mod state;
pub(crate) mod sync;

use tauri_plugin_http::reqwest::{self, Response};

pub const APP_NAME: &str = "opal-matrix";

use tauri_plugin_log::{Target, TargetKind};
use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::matrix_api::keyring::{self, StoredSession, init_keyring};
use crate::matrix_api::matrixrtc::{join_matrixrtc_call, leave_matrixrtc_call};
use crate::matrix_api::media::{get_media_from_uuid_str, get_media_from_uuid_thmubnail_str, get_member_avatar, get_room_avatar, get_user_avatar};
use crate::state::{AppState, CallAudioState, LiveKitRoomManager, MediaManager, TaskManager, TimelineManager};
use crate::sync::attach_callbacks;

pub type MatrixClientState<'a> = State<'a, RwLock<MatrixClient>>;
pub type TimelineManagerState<'a> = State<'a, TimelineManager>;

#[cfg(target_os = "linux")]
const PLATFORM: &str = "linux";
#[cfg(target_os = "windows")]
const PLATFORM: &str = "windows";
#[cfg(target_os = "macos")]
const PLATFORM: &str = "macos";
#[cfg(target_os = "android")]
const PLATFORM: &str = "android";
#[cfg(target_os = "ios")]
const PLATFORM: &str = "ios";

// Set initial display name for new devices to "Opal on <Platform>".
const DEVICE_DISPLAY_NAME: &str = formatcp!("Opal matrix on {PLATFORM}");

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
fn _send_notification(handle: &AppHandle, title: String, body: String) -> Result<(), TauriError> {
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
    fn as_info(&self) -> TauriError;
}

impl<T> AsInfo for T
where
    T: ToString,
{
    fn as_info(&self) -> TauriError {
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
    app_handle: AppHandle,
    matrix_client: State<'_, RwLock<MatrixClient>>,
) -> Result<RestoreResponse, TauriError> {
    let session_result = tokio::task::spawn_blocking(keyring::get_last_active_session)
        .await
        .expect("Keyring blocking task panicked");

    let Some(session) = session_result? else {
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

    let user_id = new_client.user_id().unwrap().to_string();

    *matrix_client.write().await = new_client;

    Ok(RestoreResponse::Success { user_id })
}

#[tauri::command(rename_all = "snake_case")]
async fn login(
    username: String,
    password: String,
    recovery_key: String,
    app_handle: AppHandle,
    matrix_client: State<'_, RwLock<MatrixClient>>,
    handle: AppHandle,
) -> Result<String, LoginError> {
    info!("Logging in new");

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
        .initial_device_display_name(DEVICE_DISPLAY_NAME)
        .send()
        .await
        .map_err(|e| {
            error!("Login failed: {:?}", e);
            LoginError::InvalidCredentials
        })?;
    log::debug!("Logged in with temporary client, fetching session info");

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
    log::debug!("Restored session on new client, starting recovery");

    new_client
        .encryption()
        .recovery()
        .recover(recovery_key.as_str())
        .await
        .map_err(|e| {
            error!("Recovery failed: {:?}", e);
            LoginError::InvalidCredentials
        })?;
    log::debug!("Recovery successful, verifying device");

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
    log::debug!("Device verified, starting sync loop");

    attach_callbacks(&new_client, &handle).await.map_err(|e| {
        error!("Failed to start sync loop: {:?}", e);
        LoginError::BackendError
    })?;
    log::debug!("Sync loop started");

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
    };

    tokio::task::spawn_blocking(move || {
        keyring::save_session(&session).map_err(|_| LoginError::BackendError)
    });

    Ok(user_id)
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

fn detect_content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        "image/jpeg"
    } else if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.starts_with(b"\x1A\x45\xDF\xA3") {
        "video/webm"
    } else if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
        "video/mp4"
    } else if bytes.starts_with(b"OggS") {
        "video/ogg"
    } else {
        "application/octet-stream"
    }
}

pub struct BrandColorsMap(pub HashMap<String, String>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    init_keyring();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
        .setup(|app: &mut App| {
            let config_dir = app.path().app_config_dir().map_err(|e| {
                std::io::Error::other(format!("Failed to resolve app config dir: {e}"))
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
                std::io::Error::other(format!("Failed to resolve app data dir: {e}"))
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
                    .level_for("libwebrtc", log::LevelFilter::Info)
                    .level_for("livekit", log::LevelFilter::Info)
                    .level_for("rustls_platform_verifier", log::LevelFilter::Off)
                    .level_for("html5ever", log::LevelFilter::Off)
                    .level_for("matrix_sdk", log::LevelFilter::Debug)
                    .level_for("matrix_sdk_base", log::LevelFilter::Debug)
                    .level_for("rustls", log::LevelFilter::Off)
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
            app.manage(TaskManager::default());
            app.manage(CallAudioState::default());
            app.manage(MediaManager::default());
            app.manage(LiveKitRoomManager::default());

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
            backend_log,
            set_room_id,
            set_frontend_focused,
            join_matrixrtc_call,
            leave_matrixrtc_call,
            // frontend commands
            frontend::messages::commit_message,
            frontend::messages::send_attachment,
            frontend::messages::edit_message,
            frontend::messages::get_timeline,
            frontend::messages::scroll_up,
            frontend::messages::toggle_reaction,
            frontend::commands::get_commands,
            frontend::profiles::get_members_for_room,
            frontend::profiles::get_user_profile,
            frontend::dialog::open_file_dialog,
            frontend::dialog::save_file_to_picked_dest,
            // matrix API commands
            matrix_api::discovery::choose_home_server,
            // matrix_api::messages::fetch_messages,
            matrix_api::account_data::get_breadcrumbs,
            matrix_api::account_data::set_breadcrumbs,
            matrix_api::account_data::get_server_order,
            matrix_api::account_data::set_server_order,
            matrix_api::previews::get_url_preview,
        ])
        .register_asynchronous_uri_scheme_protocol(
            "mxc",
            move |ctx, request: tauri::http::Request<Vec<u8>>, responder| {
                let app_handle = ctx.app_handle().clone();
                let uri = request.uri().to_string();
                let uri = percent_decode_str(&uri).decode_utf8_lossy().into_owned();

                let range_header = request
                    .headers()
                    .get("range")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("bytes="))
                    .and_then(|v| {
                        let mut parts = v.splitn(2, '-');
                        let start: usize = parts.next()?.parse().ok()?;
                        let end: usize = parts
                            .next()
                            .and_then(|e| e.parse().ok())
                            .unwrap_or(usize::MAX);
                        Some((start, end))
                    });

                tauri::async_runtime::spawn(async move {
                    let client = app_handle
                        .state::<tokio::sync::RwLock<matrix_sdk::Client>>()
                        .read()
                        .await
                        .clone();

                    let media_manager = app_handle.state::<MediaManager>();

                    if let Some(id_str) = uri.strip_prefix("mxc://media/") {
                        match get_media_from_uuid_str(&client, id_str, &media_manager).await {
                            Ok(bytes) => {
                                let content_type = detect_content_type(&bytes);
                                if content_type == "application/octet-stream" {
                                    log::debug!(
                                        "Unknown format for UUID {}, first 16 bytes: {:02x?}",
                                        id_str,
                                        &bytes[..bytes.len().min(16)]
                                    );
                                }
                                let is_video = content_type.starts_with("video/");
                                if is_video {
                                    if let Some((start, end)) = range_header
                                        && start > 0
                                    {
                                        let end = end.min(bytes.len().saturating_sub(1));
                                        let total = bytes.len();
                                        let chunk = bytes[start..=end].to_vec();
                                        responder.respond(
                                            tauri::http::Response::builder()
                                                .status(206)
                                                .header("Content-Type", content_type)
                                                .header(
                                                    "Content-Range",
                                                    format!("bytes {}-{}/{}", start, end, total),
                                                )
                                                .header("Content-Length", chunk.len().to_string())
                                                .header("Accept-Ranges", "bytes")
                                                .header("Access-Control-Allow-Origin", "*")
                                                .body(chunk)
                                                .unwrap(),
                                        );
                                    } else {
                                        let len = bytes.len();
                                        responder.respond(
                                            tauri::http::Response::builder()
                                                .status(200)
                                                .header("Content-Type", content_type)
                                                .header("Content-Length", len.to_string())
                                                .header("Accept-Ranges", "bytes")
                                                .header("Access-Control-Allow-Origin", "*")
                                                .body(bytes)
                                                .unwrap(),
                                        );
                                    }
                                } else {
                                    let len = bytes.len();
                                    responder.respond(
                                        tauri::http::Response::builder()
                                            .status(200)
                                            .header("Content-Type", content_type)
                                            .header("Content-Length", len.to_string())
                                            .header("Access-Control-Allow-Origin", "*")
                                            .body(bytes)
                                            .unwrap(),
                                    );
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to fetch media for UUID {}: {:?}", id_str, e);
                                responder.respond(
                                    tauri::http::Response::builder()
                                        .status(500)
                                        .body(Vec::<u8>::new())
                                        .unwrap(),
                                );
                            }
                        };
                    } else {
                        let res = if let Some(param_str) = uri.strip_prefix("mxc://thumbnail/") {
                            get_media_from_uuid_thmubnail_str(&client, param_str, &media_manager).await.map(Some)
                        } else if let Some(string) = uri.strip_prefix("mxc://user/") {
                            if let Some((user_id, room_id)) = string.split_once("/room/") {
                                get_member_avatar(&client, room_id, user_id).await
                            } else {
                                get_user_avatar(&client, string).await
                            }
                        } else if let Some(room_id) = uri.strip_prefix("mxc://room/") {
                            get_room_avatar(&client, room_id).await
                        } else {
                            log::error!("Invalid mxc URI format: {}", uri);
                            responder.respond(
                                tauri::http::Response::builder()
                                    .status(400)
                                    .body(Vec::new())
                                    .unwrap(),
                            );
                            return;
                        };

                        match res {
                            Ok(Some(bytes)) => {
                                let content_type = detect_content_type(&bytes);
                                responder.respond(
                                    tauri::http::Response::builder()
                                        .status(200)
                                        .header("Content-Type", content_type)
                                        .header("Content-Length", bytes.len().to_string())
                                        .header("Access-Control-Allow-Origin", "*")
                                        .body(bytes)
                                        .unwrap(),
                                );
                            }
                            Ok(None) => {
                                responder.respond(
                                    tauri::http::Response::builder()
                                        .status(404)
                                        .body(Vec::<u8>::new())
                                        .unwrap(),
                                );
                            }
                            Err(e) => {
                                log::error!("Failed to fetch media for URI {}: {:?}", uri, e);
                                responder.respond(
                                    tauri::http::Response::builder()
                                        .status(500)
                                        .body(Vec::<u8>::new())
                                        .unwrap(),
                                );
                            }
                        }
                    }
                });
            },
        )
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    info!("Initialized");
}
