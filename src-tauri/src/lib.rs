use dirs;
use log::trace;
use std::sync::Arc;

use aes::cipher::{KeyIvInit, StreamCipher};
use aes::Aes256;
use base64::engine::general_purpose;
use base64::Engine;
use bytes::Bytes;
use chrono::Local;
use log::info;
use serde::Serialize;
use tauri::{command, AppHandle, Url};
use tauri::{Manager, State};

pub mod frontend;
pub mod matrix_api;
pub mod state;
pub mod storage;

use matrix_api::authentication;

use tauri_plugin_http::reqwest::{self, Response};

type Aes256Ctr = ctr::Ctr64BE<Aes256>;

pub const APP_NAME: &str = "opal-matrix";

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use tauri_plugin_log::{Target, TargetKind};
use tauri_plugin_notification::{NotificationExt, PermissionState};

use crate::frontend::send_sidebar_update;
use crate::state::AppState;

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
    username: String,
    password: String,
    recovery_key: String,
    state: State<'_, Arc<AppState>>,
    handle: AppHandle,
) -> Result<LoginResponse, TauriError> {
    info!("Logging in new");

    // Ensure no sync iteration is still running with an old crypto machine/token pair, fixes an error only occuring on Android?
    state.stop_sync().await?;

    let server_info = state
        .home_server_info
        .read()
        .await
        .as_ref()
        .ok_or("Home server not set")?
        .clone();

    let (client_info, token) =
        authentication::matrix_login(server_info, username, password).await?;

    {
        let mut client_guard = state.client.write().await;
        let mut token_guard = state.token.write().await;

        *token_guard = Some(token.clone());
        *client_guard = Some(client_info.clone());
    }

    state.save_session().await?;

    state.init_stuff(&handle).await?;

    state.set_recovery_key(recovery_key).await?;

    Ok(LoginResponse {
        user_id: client_info.user_id,
    })
}

/// Sets the recovery key for the current user. The key is saved in the keyring.
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_foldername = format!(
        "{}/logs/{}",
        dirs::data_dir()
            .expect("Failed to get app data dir")
            .join(APP_NAME)
            .to_string_lossy(),
        Local::now().format("%Y-%m-%d")
    )
    .into();
    let log_filename = format!("{}.log", Local::now().format("%H-%M-%S")).into();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_http::init())
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

            let main_window = app.get_webview_window("main").expect("Failed to get main window");

            if cfg!(not(target_os = "android")) {
                main_window.maximize().ok();
            }

            Ok(())
        })
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Debug)
                .targets([Target::new(tauri_plugin_log::TargetKind::Stdout), Target::new(TargetKind::Folder { path: log_foldername, file_name: log_filename })])
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
                        record.line().unwrap_or(0).to_string(),
                        message
                    ));
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            login,
            fetch_raw_html,
            try_restore,
            set_recovery_key,
            send_frontend,
            backend_log,
            set_room_id,

            // frontend commands
            frontend::messages::commit_message,

            // storage commands
            storage::get_members,
            storage::members::get_members_for_room,
            storage::receipts::get_receipt,

            // matrix API commands
            matrix_api::discovery::choose_home_server,
            matrix_api::messages::fetch_messages,
            matrix_api::rooms::send_read_marker,
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

                    let matrix_url = if let Some(server) = state.home_server_info.read().await.as_ref() {
                        server.base_url.clone()
                    } else {
                        "https://matrix-client.matrix.org".to_string()
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
