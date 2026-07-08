use matrix_sdk::Client;
use notify::Watcher;
use percent_encoding::percent_decode_str;
use shared::api::events::{NotificationEvent, NotificationLevel, SettingsUpdate};
use shared::synth::ProfileAudio;
use std::collections::HashMap;
use std::fs::{read_to_string, write};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tauri::async_runtime::{block_on, spawn};
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::{RwLock, mpsc};
use toml_edit::DocumentMut;

use chrono::Local;
use log::LevelFilter;
use tauri::{App, Builder, Manager, Wry};

use tauri_plugin_cli::CliExt;
use tauri_plugin_log::{Target, TargetKind};

use crate::ipc_log::{self, IpcTrafficLog};
use crate::matrix_api::media::{
    get_direct_media, get_media_from_uuid_str, get_media_from_uuid_thmubnail_str,
    get_member_avatar, get_room_avatar, get_user_avatar,
};
use crate::state::{
    AppState, AudioManager, LiveKitRoomManager, MediaManager, TaskManager, TimelineManager,
};
use crate::{
    BrandColorsMap, TauriError, detect_content_type, diff_settings, send_event, send_event_logless,
};

use super::frontend;
use super::matrix_api;
use super::settings;

pub fn add_invoke_handler(builder: Builder<Wry>) -> Builder<Wry> {
    builder.invoke_handler(tauri::generate_handler![
        super::login,
        super::fetch_raw_html,
        super::try_restore,
        super::close_window,
        super::minimize_window,
        super::toggle_fullscreen,
        super::backend_log,
        ipc_log::log_ipc_call,
        super::set_room_id,
        super::set_frontend_focused,
        // frontend commands
        frontend::messages::commit_message,
        frontend::messages::send_attachment,
        frontend::messages::edit_message,
        frontend::messages::get_timeline,
        frontend::messages::scroll_timeline,
        frontend::messages::toggle_reaction,
        frontend::messages::delete_message,
        frontend::messages::indicate_typing,
        frontend::messages::get_pinned_events,
        frontend::commands::get_commands,
        frontend::profiles::get_user_profile,
        frontend::dialog::open_file_dialog,
        frontend::dialog::save_file_to_picked_dest,
        frontend::settings::change_screen_scaling,
        frontend::klipy::search_gifs,
        frontend::search::search_rooms,
        // matrix API commands
        matrix_api::discovery::choose_home_server,
        // matrix_api::messages::fetch_messages,
        matrix_api::account_data::get_breadcrumbs,
        matrix_api::account_data::set_breadcrumbs,
        matrix_api::account_data::get_server_order,
        matrix_api::account_data::set_server_order,
        matrix_api::previews::get_url_preview,
        matrix_api::profile::save_displayname,
        matrix_api::profile::save_namecolor,
        matrix_api::profile::save_bannercolor,
        matrix_api::profile::save_sonic_signature,
        matrix_api::matrixrtc::join_matrixrtc_call,
        matrix_api::matrixrtc::leave_matrixrtc_call,
        // settings
        settings::get_setting,
        settings::set_setting,
        settings::set_setting_cloud,
        // audio
        frontend::audio::set_output_device,
        frontend::audio::set_input_device,
        frontend::audio::get_audio_devices,
    ])
}

fn add_logging_plugin(
    app: &mut App,
    log_dir: PathBuf,
    start_time: &str,
    log_level: LevelFilter,
    livekit_log_level: LevelFilter,
    keyring_log_level: LevelFilter,
) -> Result<(), TauriError> {
    let handle = app.handle().clone();
    app.handle().plugin(
        tauri_plugin_log::Builder::new()
            .clear_targets()
            .level(log_level)
            .max_file_size(u128::MAX)
            .targets([
                Target::new(tauri_plugin_log::TargetKind::Stdout),
                Target::new(TargetKind::Folder {
                    path: log_dir,
                    file_name: Some(format!("{start_time}.log")),
                }),
            ])
            .level_for("reqwest", LevelFilter::Off)
            .level_for("libwebrtc", livekit_log_level)
            .level_for("livekit", livekit_log_level)
            .level_for("matrix_sdk", LevelFilter::max())
            .level_for("rustls_platform_verifier", LevelFilter::Off)
            .level_for("html5ever", LevelFilter::Off)
            .level_for("matrix_sdk_base", LevelFilter::Debug)
            .level_for("rustls", LevelFilter::Off)
            .level_for("keyring_core", keyring_log_level)
            .level_for("zbus_secret_service_keyring_store", keyring_log_level)
            .level_for("apple_native_keyring_store", keyring_log_level)
            .level_for("android_native_keyring_store", keyring_log_level)
            .level_for("windows_native_keyring_store", keyring_log_level)
            .level_for("tantivy", LevelFilter::Warn)
            .format(move |out, message, record| {
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

                match record.level() {
                    log::Level::Error => send_event_logless(
                        &handle.clone(),
                        &NotificationEvent::GenericNotification {
                            title: "Error".to_string(),
                            message: message.to_string(),
                            level: NotificationLevel::Error,
                        },
                    ),
                    log::Level::Warn => send_event_logless(
                        &handle.clone(),
                        &NotificationEvent::GenericNotification {
                            title: "Warning".to_string(),
                            message: message.to_string(),
                            level: NotificationLevel::Warning,
                        },
                    ),
                    _ => {}
                };

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

    Ok(())
}

pub fn register_mxc_uri(builder: Builder<Wry>) -> Builder<Wry> {
    builder.register_asynchronous_uri_scheme_protocol(
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
                        get_media_from_uuid_thmubnail_str(&client, param_str, &media_manager)
                            .await
                            .map(Some)
                    } else if let Some(string) = uri.strip_prefix("mxc://user/") {
                        if let Some((user_id, room_id)) = string.split_once("/room/") {
                            get_member_avatar(&client, room_id, user_id).await
                        } else {
                            get_user_avatar(&client, string).await
                        }
                    } else if let Some(room_id) = uri.strip_prefix("mxc://room/") {
                        get_room_avatar(&client, room_id).await
                    } else {
                        get_direct_media(&client, &uri).await
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
}

pub fn setup_builder(builder: Builder<Wry>) -> Builder<Wry> {
    builder.setup(|app: &mut App| {
        let config_dir = app.path().app_config_dir().map_err(|e| {
            std::io::Error::other(format!("Failed to resolve app config dir: {e}"))
        })?;

        let state = Arc::new(AppState::default());

        let clone = app.handle().clone();
        let update_state = state.clone();
        spawn(async move {
            let Ok(updater) = clone.updater() else {
                log::error!("Failed to get updater");
                return;
            };
            let Ok(maybe_update) = updater.check().await else {
                log::error!("Failed to check for updates");
                return;
            };
            if let Some(update) = maybe_update {
                log::info!("Update available: {:?}, {:?}, {:?}, {:?}", update.body, update.current_version, update.current_version, update.download_url);

                send_event(&clone, &NotificationEvent::UpdateAvailable);

                *update_state.update.write().await = Some(update.clone());
            } else {
                log::info!("No update available");
            }
        });
        app.manage(state);

        std::fs::create_dir_all(&config_dir)?;

        let settings_file_path = config_dir.join("settings.toml");

        let settings_doc = match read_to_string(&settings_file_path) {
            Ok(content) => match DocumentMut::from_str(&content) {
                Ok(doc) => doc,
                Err(e) => {
                    log::warn!("Failed to parse settings file, starting with empty settings: {:?}", e);
                    DocumentMut::new()
                }
            },
            Err(e) => {
                log::warn!("Failed to read settings file, starting with empty settings: {:?}", e);
                DocumentMut::new()
            },
        };

        let app_handle = app.handle().clone();
        let watch_path = settings_file_path.clone();

        spawn(async move {
        let (tx, mut rx) = mpsc::channel::<()>(100);

        let watch_path_for_filter = watch_path.clone();
        let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res
                && !matches!(event.kind, notify::EventKind::Access(_))
                && event.paths.iter().any(|p| p == &watch_path_for_filter)
            {
                let _ = tx.blocking_send(());
            }
        }).expect("Failed to initialize file watcher");

        let watch_dir = watch_path.parent().expect("Settings file has no parent directory");
        watcher.watch(watch_dir, notify::RecursiveMode::NonRecursive).expect("Failed to watch settings directory");

        while rx.recv().await.is_some() {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            while rx.try_recv().is_ok() {}

            let Ok(new_content) = tokio::fs::read_to_string(&watch_path).await else {
                log::warn!("Failed to read settings file after change");
                continue;
            };

            let cashed_settings_sig = app_handle.state::<RwLock<DocumentMut>>().clone();
            let mut cached_settings = cashed_settings_sig.write().await;

            if new_content == cached_settings.to_string() {
                continue;
            }

            match DocumentMut::from_str(&new_content) {
                Ok(new_doc) => {
                    let changed_keys = diff_settings(&cached_settings, &new_doc);
                    *cached_settings = new_doc;
                    let json_map = super::settings::document_to_json_map(&cached_settings);
                    for key in &changed_keys {
                        let json_str = json_map
                            .get(key)
                            .and_then(|v| serde_json::to_string(v).ok())
                            .unwrap_or_else(|| "null".to_string());

                        log::info!("Settings file changed");
                        send_event(&app_handle, &SettingsUpdate {
                            key: key.clone(),
                            value: json_str.clone(),
                            cloud: false,
                        });
                    };
                }
                Err(e) => {
                    log::warn!("Failed to parse settings file after change, keeping old settings: {:?}", e);
                }
            }
        }

        drop(watcher);
        });

        app.manage(settings_file_path);
        app.manage(RwLock::new(settings_doc));

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
        // let log_file = format!("{}.log", Local::now().format("%H-%M-%S"));

        let cli_matches = app.cli().matches().ok();
        let cli_log_level = |arg_name: &str| -> Option<log::LevelFilter> {
            let value = cli_matches.as_ref()?.args.get(arg_name)?.value.as_str()?;

            match value {
                "trace" => Some(log::LevelFilter::Trace),
                "debug" => Some(log::LevelFilter::Debug),
                "info" => Some(log::LevelFilter::Info),
                "warn" => Some(log::LevelFilter::Warn),
                "error" => Some(log::LevelFilter::Error),
                "off" => Some(log::LevelFilter::Off),
                _ => None,
            }
        };

        let log_level = cli_log_level("log-level").unwrap_or(log::LevelFilter::Debug);
        let livekit_log_level =
            cli_log_level("livekit-log-level").unwrap_or(log::LevelFilter::Off);
        let keyring_log_level =
            cli_log_level("keyring-log-level").unwrap_or(log::LevelFilter::Off);

        let start_time = Local::now().format("%H-%M-%S").to_string();

        #[cfg(desktop)]
        match IpcTrafficLog::init(&log_dir, &start_time) {
            Ok(ipc_traffic_log) => {
                app.manage(ipc_traffic_log);
            }
            Err(e) => eprintln!("Failed to initialize IPC traffic log: {:?}", e),
        }

        add_logging_plugin(app, log_dir, &start_time, log_level, livekit_log_level, keyring_log_level).map_err(|e| {
            std::io::Error::other(format!("Failed to initialize logging plugin: {:?}", e))
        })?;

        let client = block_on(async {
            Client::builder()
                .handle_refresh_tokens()
                .homeserver_url("https://matrix.org")
                .build()
                .await
                .unwrap()
        });

        app.manage(RwLock::new(client));
        app.manage(TimelineManager::default());
        app.manage(TaskManager::default());
        app.manage(MediaManager::default());
        app.manage(LiveKitRoomManager::default());
        app.manage(AudioManager::new(app.handle().clone()));
        app.manage(ProfileAudio::default());

        #[cfg(not(target_os = "android"))]
        let main_window = app
            .get_webview_window("main")
            .expect("Failed to get main window");

        #[cfg(not(target_os = "android"))]
        main_window.maximize().ok();

        Ok(())
    })
}
