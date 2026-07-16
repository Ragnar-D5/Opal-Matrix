use matrix_sdk::Client;
use notify::Watcher;
use shared::api::UpdateStatus;
use shared::api::events::{
    LogEntry, NotificationEvent, NotificationLevel, SettingsUpdate, TauriEvent,
};
use std::collections::HashMap;
use std::fs::{read_to_string, write};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tauri::async_runtime::{block_on, spawn};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::{RwLock, mpsc};
use toml_edit::DocumentMut;

use chrono::Local;
use log::LevelFilter;
use tauri::{App, AppHandle, Builder, Emitter, Manager, Wry};

use tauri_plugin_cli::CliExt;
use tauri_plugin_log::{Target, TargetKind};

#[cfg(all(desktop, debug_assertions))]
use crate::ipc_log::IpcTrafficLog;
use crate::state::{
    AppState, AudioManager, LiveKitRoomManager, LogBuffer, RoomSearchManager, TaskManager,
    TimelineManager,
};
use crate::{BrandColorsMap, TauriError, diff_settings, send_event, send_event_logless};
use crate::{check_for_update_backend, info_from_update, ipc_log};

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
        super::open_log_window,
        super::get_log_backlog,
        ipc_log::log_ipc_call,
        super::set_room_id,
        super::set_frontend_focused,
        // updates
        super::check_for_update,
        super::get_update_status,
        super::download_update,
        super::install_update,
        super::get_app_version,
        super::recheck_update,
        // versions
        super::versions::get_version,
        super::versions::get_versions,
        // frontend commands
        frontend::messages::send_message,
        frontend::messages::send_attachment,
        frontend::messages::edit_message,
        frontend::messages::get_timeline,
        frontend::messages::scroll_timeline,
        frontend::messages::toggle_reaction,
        frontend::messages::delete_message,
        frontend::messages::indicate_typing,
        frontend::messages::get_pinned_events,
        frontend::messages::pin_event,
        frontend::messages::unpin_event,
        frontend::commands::get_commands,
        frontend::profiles::get_user_profile,
        frontend::dialog::open_file_dialog,
        frontend::dialog::save_file_to_picked_dest,
        frontend::settings::change_screen_scaling,
        frontend::klipy::search_gifs,
        frontend::search::search_rooms,
        frontend::rooms::get_extra_room_info,
        // matrix API commands
        matrix_api::discovery::choose_home_server,
        matrix_api::media::get_file,
        matrix_api::media::get_thumbnail,
        matrix_api::media::get_user_avatar,
        matrix_api::media::get_room_avatar,
        matrix_api::media::get_member_avatar,
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
        matrix_api::rooms::open_room_search,
        matrix_api::rooms::search_room_directory,
        matrix_api::rooms::load_more_room_search_results,
        matrix_api::rooms::close_room_search,
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
            .level_for("tauri_plugin_updater", LevelFilter::Warn)
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

                let file = record.file().unwrap_or("Unknown");
                let line_no = record.line().unwrap_or(0);

                // Buffer the line and, if the log window is open, stream it live.
                // Buffering always happens so the window shows the full backlog
                // whenever it is (re)opened. A raw `emit_to` is used rather than
                // `send_event` to avoid recursing back through the logger.
                if let Some(buffer) = handle.try_state::<LogBuffer>() {
                    let entry = buffer.push(
                        level.to_string(),
                        time.clone(),
                        file.to_string(),
                        line_no,
                        message.to_string(),
                    );
                    if handle.get_webview_window("logs").is_some() {
                        let _ = handle.emit_to("logs", LogEntry::name().as_str(), &entry);
                    }
                }

                out.finish(format_args!(
                    "{}|{}|{}:{}|{}",
                    level, time, file, line_no, message
                ));
            })
            .build(),
    )?;

    Ok(())
}

fn toggle_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_visible().unwrap_or(false) {
            win.hide().ok();
        } else {
            win.show().ok();
            win.unminimize().ok();
            win.set_focus().ok();
        }
    }
}

pub fn setup_builder(builder: Builder<Wry>) -> Builder<Wry> {
    builder.setup(|app: &mut App| {
        let config_dir = app.path().app_config_dir().map_err(|e| {
            std::io::Error::other(format!("Failed to resolve app config dir: {e}"))
        })?;

        let show_hide = MenuItem::with_id(app, "show_hide", "Show/Hide Opal", true, None::<&str>).map_err(|e| format!("Failed to set up show/hide menu: {e}"))?;
        let updates = MenuItem::with_id(app, "check_updates", "Check for Updates", true, None::<&str>).map_err(|e| format!("Failed to set up updates menu: {e}"))?;
        let quit = MenuItem::with_id(app, "quit", "Close Opal", true, None::<&str>).map_err(|e| format!("Failed to set up quit menu: {e}"))?;
        let menu = Menu::with_items(app, &[&show_hide, &updates, &quit]).map_err(|e| format!("Failed to setup tray icon: {e}"))?;

        let mut tray: TrayIconBuilder<Wry> = TrayIconBuilder::new().menu(&menu).show_menu_on_left_click(false).on_menu_event(|app, event| match event.id.as_ref() {
            "show_hide" => toggle_main_window(app),
            "check_updates" => {
                let app = app.clone();
                log::info!("Checking for updates...");
                spawn(async move {
                    let state = app.state::<Arc<AppState>>();
                    check_for_update_backend(state.clone(), app.clone()).await.ok();
                });
            },
            "quit" => app.exit(0),
            _ => {}
        });


        if let Some(icon) = app.default_window_icon() {
            tray = tray.icon(icon.clone());
        }

        tray.build(app)?;

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
                *update_state.update_status.write().await = UpdateStatus::UpdateAvailable(info_from_update(&update));
            } else {
                log::info!("No update available");
                *update_state.update_status.write().await = UpdateStatus::UpToDate;
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
                            skip_cloud_upload: false,
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

        #[cfg(all(desktop, debug_assertions))]
        match IpcTrafficLog::init(&log_dir, &start_time) {
            Ok(ipc_traffic_log) => {
                app.manage(ipc_traffic_log);
            }
            Err(e) => eprintln!("Failed to initialize IPC traffic log: {:?}", e),
        }

        // Managed before the logging plugin so the format closure can find it.
        app.manage(LogBuffer::default());

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
        app.manage(LiveKitRoomManager::default());
        app.manage(AudioManager::new(app.handle().clone()));
        app.manage(RoomSearchManager::default());

        #[cfg(not(target_os = "android"))]
        let main_window = app
            .get_webview_window("main")
            .expect("Failed to get main window");

        #[cfg(not(target_os = "android"))]
        main_window.maximize().ok();

        Ok(())
    })
}
