#![recursion_limit = "256"]

use const_format::formatcp;
use log::{error, trace};
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::encryption::{BackupDownloadStrategy, EncryptionSettings};
use matrix_sdk::ruma::{OwnedDeviceId, UserId};
use matrix_sdk::search_index::SearchIndexStoreKind;
use matrix_sdk::{
    AuthSession, Client as MatrixClient, SessionMeta, SessionTokens, SqliteStoreConfig,
};
use shared::api::RestoreResponse;
use shared::api::errors::LoginError;
use shared::api::events::TauriEvent;
use shared::synth::ProfileAudio;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use toml_edit::DocumentMut;

use bytes::Bytes;
use log::info;
use tauri::{AppHandle, Emitter, Url, command};
use tauri::{Manager, State};

pub mod builder;
pub(crate) mod frontend;
pub(crate) mod ipc_log;
pub(crate) mod matrix_api;
pub(crate) mod settings;
pub(crate) mod state;
pub(crate) mod sync;

use tauri_plugin_http::reqwest::{self, Response};

pub const APP_NAME: &str = "opal-matrix";

use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::builder::{add_invoke_handler, register_mxc_uri, setup_builder};
use crate::matrix_api::keyring::{self, StoredSession, get_or_create_store_key, init_keyring};
use crate::state::{AppState, TimelineManager};
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

pub fn send_event<T: TauriEvent>(app_handle: &AppHandle, payload: &T) {
    let bytes = serde_json::to_vec(payload).map(|b| b.len()).unwrap_or(0);
    ipc_log::log_event(app_handle, T::name().as_str(), bytes);

    if let Err(e) = app_handle.emit(T::name().as_str(), payload) {
        error!("Failed to emit event {}: {:?}", T::name(), e);
    }
}

/// Like `send_event`, but without logging to prevent possible recursion
pub fn send_event_logless<T: TauriEvent>(app_handle: &AppHandle, payload: &T) {
    let _ = app_handle.emit(T::name().as_str(), &payload);
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
    default_audio: State<'_, ProfileAudio>,
) -> Result<RestoreResponse, TauriError> {
    let session_result = tokio::task::spawn_blocking(keyring::get_last_active_session)
        .await
        .expect("Keyring blocking task panicked");

    let Some(session) = session_result? else {
        return Ok(RestoreResponse::NoSession);
    };

    let user_id = session.user_id;
    let safe_user_id = user_id.replace(':', "_");
    let device_id = session.device_id;

    let data_dir = app_handle.path().app_data_dir()?;

    let path = data_dir
        .join("sessions")
        .join(format!("{}-{}.db", safe_user_id, &device_id));

    let cache_path = app_handle
        .path()
        .app_cache_dir()?
        .join("sessions_cache")
        .join(format!("{}-{}", safe_user_id, &device_id));

    let index_path = data_dir
        .join("sessions_index")
        .join(format!("{}-{}", safe_user_id, &device_id));

    std::fs::create_dir_all(&cache_path).unwrap_or_default();

    let store_key: [u8; 32] = get_or_create_store_key(&user_id).await?;
    let sqlite_store_config = SqliteStoreConfig::new(path).key(Some(&store_key));

    let password = hex::encode(store_key);
    let new_client = MatrixClient::builder()
        .homeserver_url(session.homeserver_url.clone())
        .handle_refresh_tokens()
        .sqlite_store_with_config_and_cache_path(sqlite_store_config, Some(cache_path))
        .search_index_store(SearchIndexStoreKind::EncryptedDirectory(
            index_path, password,
        ))
        .with_encryption_settings(EncryptionSettings {
            backup_download_strategy: BackupDownloadStrategy::AfterDecryptionFailure,
            ..Default::default()
        })
        .build()
        .await?;

    let session = AuthSession::Matrix(MatrixSession {
        meta: SessionMeta {
            device_id: OwnedDeviceId::from(device_id),
            user_id: UserId::parse(user_id)?,
        },
        tokens: SessionTokens {
            access_token: session.access_token,
            refresh_token: session.refresh_token,
        },
    });
    new_client.restore_session(session).await?;

    attach_callbacks(&new_client, &app_handle, default_audio.inner()).await?;

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
    default_audio: State<'_, ProfileAudio>,
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

    let safe_user_id = user_id.to_string().replace(':', "_");

    let data_dir = app_handle.path().app_data_dir().map_err(|e| {
        log::error!("Failed to get app data dir: {:?}", e);
        LoginError::BackendError
    })?;

    let path = data_dir
        .join("sessions")
        .join(format!("{}-{}.db", safe_user_id, device_id));

    let cache_path = app_handle
        .path()
        .app_cache_dir()
        .expect("Failed to get app cache dir")
        .join("sessions_cache")
        .join(format!("{}-{}", safe_user_id, device_id));

    let index_path = data_dir
        .join("sessions_index")
        .join(format!("{}-{}", safe_user_id, device_id));

    std::fs::create_dir_all(&cache_path).unwrap_or_default();

    let store_key = get_or_create_store_key(user_id.as_str())
        .await
        .map_err(|e| {
            error!("Failed to get or create store key: {:?}", e);
            LoginError::BackendError
        })?;
    let sqlite_store_config = SqliteStoreConfig::new(path).key(Some(&store_key));

    let password = hex::encode(store_key);
    let new_client = MatrixClient::builder()
        .homeserver_url(
            Url::parse(server_url.as_str()).expect("Valid homeserverurl from other client"),
        )
        .handle_refresh_tokens()
        .sqlite_store_with_config_and_cache_path(sqlite_store_config, Some(cache_path))
        .search_index_store(SearchIndexStoreKind::EncryptedDirectory(
            index_path, password,
        ))
        .with_encryption_settings(EncryptionSettings {
            backup_download_strategy: BackupDownloadStrategy::AfterDecryptionFailure,
            ..Default::default()
        })
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

    attach_callbacks(&new_client, &handle, default_audio.inner())
        .await
        .map_err(|e| {
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
async fn close_window(app_handle: AppHandle) -> Result<(), TauriError> {
    app_handle
        .get_webview_window("main")
        .ok_or("Window not found")?
        .close()?;
    Ok(())
}

#[command]
async fn minimize_window(app_handle: AppHandle) -> Result<(), TauriError> {
    app_handle
        .get_webview_window("main")
        .ok_or("Window not found")?
        .minimize()?;
    Ok(())
}

#[command]
async fn toggle_fullscreen(app_handle: AppHandle) -> Result<(), TauriError> {
    let window = app_handle
        .get_webview_window("main")
        .ok_or("Window not found")?;
    if window.is_fullscreen()? || window.is_maximized()? {
        window.set_fullscreen(false)?;
        window.unmaximize()?;
    } else {
        window.set_fullscreen(true)?;
    }
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
    _timestamp: String,
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

    log::logger().log(
        &log::Record::builder()
            .args(format_args!("{}", message))
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

fn diff_settings(old: &DocumentMut, new: &DocumentMut) -> Vec<String> {
    let mut changed = Vec::new();
    for (key, new_val) in new.iter() {
        match old.get(key) {
            Some(old_val) if old_val.to_string() == new_val.to_string() => {}
            _ => changed.push(key.to_string()),
        }
    }
    for (key, _) in old.iter() {
        if new.get(key).is_none() {
            changed.push(key.to_string());
        }
    }
    changed
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    init_keyring();

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_cli::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init());

    builder = setup_builder(builder);
    builder = add_invoke_handler(builder);
    builder = register_mxc_uri(builder);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    info!("Initialized");
}
