use crate::components::authentication::{Authentication, get_stuff_after_login};
use crate::components::loading::Loading;
use crate::components::previews::ImageLightbox;
use crate::components::settings::Settings;
use crate::components::shader::BackgroundShader;
use chrono::{DateTime, Local};
use ruma::OwnedRoomId;
use shared::api::events::{
    CallMemberUpdate, NotificationEvent, NotificationLevel, NotificationUpdate, PresenceUpdate,
    ProfileUpdates, RecentEmojies, RoomPinnedUpdate, TypingUpdate,
};
use shared::api::{AudioDeviceInfos, RestoreResponse, UpdateDownloadProgress, UpdateStatus};
use shared::profile::UserProfile;
use shared::settings::{DataSizeUnit, DateFormat, HourFormat};
use shared::sidebar::{DmList, RoomMapUpdate, ServerList, SingleList};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use log::error;

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::account_data::ServerOrder;
use wasm_bindgen::prelude::*;
use web_sys::HtmlImageElement;

use crate::components::{
    SystemButtons,
    chat::Chat,
    overlays::emoji_picker::{EmojiPickerPortal, EmojiPickerState},
    overlays::gif_picker::{GifPickerPortal, GifPickerState},
    overlays::profile_card::{ProfileCardPortal, ProfileCardState},
    overlays::space_search::{SpaceSearchPortal, SpaceSearchState},
    sidebar::Sidebar,
};
use crate::hooks::{call_tauri_no_args, setup_update_effect, use_tauri_event};
use crate::redact_mode::{self, REDACTION_ROOT_SELECTOR};
use crate::state::{AppState, MediaCache, ProfileStore};
use crate::tauri_functions::{
    change_screen_scaling, get_app_version, get_update_status, set_backend_room_id,
    set_focused_in_backend,
};

#[derive(Clone, Debug, Copy, PartialEq, Default)]
pub enum CurrentWindow {
    HomeserverDiscovery,
    Login,
    Home,
    #[default]
    Loading,
}

pub fn format_date(date: DateTime<Local>) -> Memo<String> {
    let settings: Settings = expect_context();
    let timezone_sig = settings.timezone.signal();
    let hour_format_sig = settings.hour_format.signal();
    let date_format_sig = settings.date_format.signal();

    Memo::new(move |_| {
        let timezone = timezone_sig.get();
        let date = date.with_timezone(&timezone);
        let now = Local::now().with_timezone(&timezone);

        let hour_str = match hour_format_sig.get() {
            HourFormat::TwelveHour => "%I:%M %p",
            HourFormat::TwentyFourHour => "%H:%M",
        };
        let date_str = match date_format_sig.get() {
            DateFormat::DayMonthYear => "%d/%m/%Y",
            DateFormat::MonthDayYear => "%m/%d/%Y",
            DateFormat::YearMonthDay => "%Y/%m/%d",
        };

        match (date.date_naive() - now.date_naive()).num_days() {
            0 => date.format(&format!("Today, {}", hour_str)).to_string(),
            -1 => date.format(&format!("Yesterday, {}", hour_str)).to_string(),
            -6..-1 => date
                .format(&format!("%a {}, {}", date_str, hour_str))
                .to_string(),
            _ => date
                .format(&format!("{}, {}", date_str, hour_str))
                .to_string(),
        }
    })
}

pub fn format_time(date: DateTime<Local>) -> Memo<String> {
    let settings: Settings = expect_context();
    let timezone_sig = settings.timezone.signal();
    let hour_format_sig = settings.hour_format.signal();

    Memo::new(move |_| {
        let date = date.with_timezone(&timezone_sig.get());
        let hour_str = match hour_format_sig.get() {
            HourFormat::TwelveHour => "%I:%M %p",
            HourFormat::TwentyFourHour => "%H:%M",
        };
        date.format(hour_str).to_string()
    })
}

pub fn format_bytes(bytes: u64) -> Memo<String> {
    let settings: Settings = expect_context();

    Memo::new(move |_| {
        let (size, units): (f64, [&str; 5]) = match settings.data_size_unit.signal().get() {
            DataSizeUnit::Bytes => (bytes as f64, ["B", "KB", "MB", "GB", "TB"]),
            DataSizeUnit::Mibibytes => (bytes as f64, ["B", "KiB", "MiB", "GiB", "TiB"]),
            DataSizeUnit::Bits => (bytes as f64 * 8.0, ["b", "Kb", "Mb", "Gb", "Tb"]),
        };

        let mut size = size;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < units.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", size as u64, units[unit_index])
        } else {
            format!("{:.2} {}", size, units[unit_index])
        }
    })
}

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::default();
    provide_context(state);

    provide_context(MediaCache::default());

    let settings = Settings::default();
    settings.setup_backend_hook();

    let scaling_sig = settings.scaling.signal();

    Effect::new(move |_| {
        change_screen_scaling(scaling_sig.get());
    });

    provide_context(settings);

    let store = ProfileStore::default();
    provide_context(store);

    let last_window = RwSignal::new(state.current_window.get_untracked());

    Effect::new(move |_| {
        let room_id = state.active_room_id();

        spawn_local(async move {
            if let Err(e) = set_backend_room_id(room_id).await {
                error!("Error setting backend room id: {:?}", e);
            };
        });
    });

    Effect::new(move |_| {
        let _ = state.active_room_id();
        redact_mode::set_redaction_mode(
            settings.epstein_mode.signal().get(),
            REDACTION_ROOT_SELECTOR,
        );
    });

    Effect::new(move |_| {
        let current = state.current_window.get();
        let previous = last_window.get_untracked();

        if current != previous {
            if let Some(perf) = web_sys::window().and_then(|w| w.performance()) {
                state.last_changed_time.set(perf.now() / 1000.0);
            }
            state.previous_window.set(previous);
            last_window.set(current);
        }
    });

    let profile_updates: ReadSignal<Option<ProfileUpdates>> = use_tauri_event();

    setup_update_effect(profile_updates, move |updates| {
        for (room_id, profiles) in updates {
            for profile in profiles {
                // `maybe_update` (returning `false` when nothing changed)
                // avoids notifying subscribers when the backend re-sends a
                // profile that hasn't actually changed, which would
                // otherwise tear down and re-fetch avatars downstream.
                store
                    .get_member_profile(&room_id, &profile.profile.user_id)
                    .maybe_update(|current| {
                        if current.profile.avatar_url.is_none() {
                            log::warn!("No avatar URL for user {}", profile.profile.user_id);
                        }

                        if *current != profile {
                            *current = profile.clone();
                            true
                        } else {
                            false
                        }
                    });
            }
        }
    });

    // The backend pushes the logged-in user's own global profile eagerly during
    // login/restore, rather than waiting for something to lazily call
    // `get_user_profile`.
    let own_profile_update: ReadSignal<Option<UserProfile>> = use_tauri_event();

    setup_update_effect(own_profile_update, move |profile| {
        store
            .get_user_profile(&profile.user_id)
            .maybe_update(|current| {
                if *current != profile {
                    *current = profile;
                    true
                } else {
                    false
                }
            });
    });

    Effect::new(move |_| {
        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };

        let on_focus = Closure::<dyn FnMut()>::new(move || state.is_focused.set(true));
        let on_blur = Closure::<dyn FnMut()>::new(move || state.is_focused.set(false));

        window
            .add_event_listener_with_callback("focus", on_focus.as_ref().unchecked_ref())
            .ok();
        window
            .add_event_listener_with_callback("blur", on_blur.as_ref().unchecked_ref())
            .ok();

        on_focus.forget();
        on_blur.forget();
    });

    Effect::new(move |_| {
        let focused = state.is_focused.get();

        spawn_local(async move {
            if let Err(e) = set_focused_in_backend(focused).await {
                error!("Error setting focus in backend: {:?}", e);
            }
        });
    });

    let presence_update: ReadSignal<Option<PresenceUpdate>> = use_tauri_event();

    setup_update_effect(presence_update, move |updates| {
        for (user_id, presence) in updates.iter() {
            store.get_presence(user_id).set(presence.clone());
        }
    });

    let notification_counts_update: ReadSignal<Option<NotificationUpdate>> = use_tauri_event();

    setup_update_effect(notification_counts_update, move |new| {
        state
            .notification_counts
            .update(|counts| counts.extend(new));
    });

    let call_member_update: ReadSignal<Option<CallMemberUpdate>> = use_tauri_event();

    setup_update_effect(call_member_update, move |new| {
        state.update_call_members(new);
    });

    let typing_update: ReadSignal<Option<TypingUpdate>> = use_tauri_event();

    setup_update_effect(typing_update, move |update| {
        state.update_typing_users(&update.room_id, update.user_ids);
    });

    let audio_device_update: ReadSignal<Option<AudioDeviceInfos>> = use_tauri_event();

    setup_update_effect(audio_device_update, move |update| {
        state.audio_devices.set(update);
    });

    let recent_emoji_update: ReadSignal<Option<RecentEmojies>> = use_tauri_event();

    setup_update_effect(recent_emoji_update, move |update| {
        state.recent_emojies.set(update);
    });

    let download_update: ReadSignal<Option<UpdateDownloadProgress>> = use_tauri_event();

    setup_update_effect(download_update, move |update| {
        state.update_progress.set(update);
    });

    let update_status: ReadSignal<Option<UpdateStatus>> = use_tauri_event();

    setup_update_effect(update_status, move |update| {
        state.update_status.set(update);
    });

    get_update_status();

    spawn_local(async move {
        match get_app_version().await {
            Ok(ver) => state.app_version.set(ver),
            Err(e) => log::error!("Failed to get version: {e}"),
        }
    });

    let room_map_event: ReadSignal<Option<Vec<RoomMapUpdate>>> = use_tauri_event();
    let dm_list_event: ReadSignal<Option<DmList>> = use_tauri_event();
    let single_room_list_event: ReadSignal<Option<SingleList>> = use_tauri_event();
    let server_list_event: ReadSignal<Option<ServerList>> = use_tauri_event();

    setup_update_effect(room_map_event, move |new| {
        let mut room_map = state.room_map.get_untracked();

        for update in new {
            match update {
                RoomMapUpdate::Insert { key, value } => {
                    if let Some(sig) = room_map.get(&key) {
                        sig.set(value);
                    } else {
                        room_map.insert(key, ArcRwSignal::new(value));
                    }
                }
                RoomMapUpdate::Remove { key } => {
                    room_map.remove(&key);
                }
                RoomMapUpdate::Set { map } => {
                    room_map.clear();
                    for (room_id, node) in map {
                        room_map.insert(room_id, ArcRwSignal::new(node));
                    }
                }
            }
        }

        state.room_map.set(room_map);
    });

    setup_update_effect(dm_list_event, move |new| {
        state.dm_list.set(new);
    });

    setup_update_effect(single_room_list_event, move |new| {
        state.single_room_list.set(new);
    });

    let pin_update: ReadSignal<Option<RoomPinnedUpdate>> = use_tauri_event();

    setup_update_effect(
        pin_update,
        move |RoomPinnedUpdate((room_id, pinned_ids))| {
            state.pinned_map.update(|map| {
                if map.get(&room_id) != Some(&pinned_ids) {
                    map.insert(room_id, pinned_ids);
                }
            });
        },
    );

    Effect::new(move |_| {
        if let Some(mut new_state) = server_list_event.get() {
            let current_order = state.server_order.get_untracked();

            let order_map: HashMap<OwnedRoomId, usize> = current_order
                .servers
                .iter()
                .enumerate()
                .map(|(index, id)| (id.clone(), index))
                .collect();

            new_state.0.sort_by(|a, b| {
                let pos_a = order_map.get(a).copied().unwrap_or(usize::MAX);
                let pos_b = order_map.get(b).copied().unwrap_or(usize::MAX);

                pos_a.cmp(&pos_b)
            });

            let final_order = new_state.0.clone();

            if final_order != current_order.servers {
                state.server_order.set(ServerOrder {
                    servers: final_order,
                });
            }

            state.server_list.set(new_state);
            state.update_active_room();
        }
    });

    Effect::new(move |_| {
        spawn_local(async move {
            match call_tauri_no_args("try_restore").await {
                Ok(js_val) => {
                    let response: RestoreResponse = serde_wasm_bindgen::from_value(js_val).unwrap();

                    match response {
                        RestoreResponse::NoSession => {
                            state.current_window.set(CurrentWindow::HomeserverDiscovery);
                        }
                        RestoreResponse::Success { user_id } => {
                            state.user_id.set(Some(user_id));
                            state.current_window.set(CurrentWindow::Home);
                            get_stuff_after_login(state, settings);
                        }
                        RestoreResponse::Failed { home_server: _ } => {
                            state.current_window.set(CurrentWindow::Login);
                        }
                    }
                }
                Err(_) => {
                    error!("Error during restore, showing home server discovery");
                }
            }
        });
    });

    view! {
        <BackgroundShader />
        {move || match state.current_window.get() {
            CurrentWindow::HomeserverDiscovery | CurrentWindow::Login => {
                view! { <Authentication /> }.into_any()
            }
            CurrentWindow::Home => view! { <HomePage /> }.into_any(),
            CurrentWindow::Loading => Loading().into_any(),
        }}
        <Notifications />
    }
}

#[component]
fn HomePage() -> impl IntoView {
    let (bg_loaded, set_bg_loaded) = signal(false);
    let bg_url = "https://i.imgur.com/t9plvkd.png".to_string();

    let bg_url_clone = bg_url.clone();
    Effect::new(move |_| {
        let img = HtmlImageElement::new().unwrap();
        let onload = Closure::<dyn FnMut()>::new(move || set_bg_loaded.set(true));
        img.set_onload(Some(onload.as_ref().unchecked_ref()));
        onload.forget();
        img.set_src(&bg_url_clone);
    });

    let root_css_vars = move || {
        let base = "cursor: default; line-height: 22px".to_string();

        if bg_loaded.get() {
            format!(
                "{base}
            background-image: url('{}');
            background-size: cover;
            background-position: center;
            background-repeat: no-repeat",
                bg_url
            )
        } else {
            base
        }
    };

    let emoji_picker_state = EmojiPickerState::default();
    provide_context(emoji_picker_state);

    let gif_picker_state = GifPickerState::default();
    provide_context(gif_picker_state);

    let profile_card_state = ProfileCardState::default();
    provide_context(profile_card_state);

    let space_search_state = SpaceSearchState::default();
    provide_context(space_search_state);

    view! {
        <div
            class="bg-transparent flex h-screen overflow-hidden p-[var(--gap)] gap-[var(--gap)] relative"
            style=root_css_vars
        >
            <div data-tauri-drag-region class="absolute top-0 left-0 right-0 h-3 z-50"></div>
            <SystemButtons active=true class="absolute top-[var(--gap)] right-[var(--gap)]" />
            <Sidebar />
            <Chat />
            <ImageLightbox />
            <EmojiPickerPortal />
            <GifPickerPortal />
            <ProfileCardPortal />
            <SpaceSearchPortal />
        </div>
    }
}

#[component]
pub fn Notifications() -> impl IntoView {
    #[derive(Clone, PartialEq)]
    struct ActiveNotification {
        id: u64,
        event: NotificationEvent,
    }

    static NOTIFICATION_COUNTER: AtomicU64 = AtomicU64::new(0);

    let active_notifications: RwSignal<Vec<ActiveNotification>> = RwSignal::new(Vec::new());

    let notification_update: ReadSignal<Option<NotificationEvent>> = use_tauri_event();

    setup_update_effect(notification_update, move |notification| {
        let id = NOTIFICATION_COUNTER.fetch_add(1, Ordering::Relaxed);

        active_notifications.update(|buf| {
            buf.push(ActiveNotification {
                id,
                event: notification,
            });
        });

        set_timeout(
            move || {
                active_notifications.update(|buf| {
                    buf.retain(|n| n.id != id);
                });
            },
            std::time::Duration::from_millis(7000),
        );
    });

    view! {
        <div class="fixed bottom-4 right-4 z-50 flex flex-col-reverse gap-3 max-w-sm w-full pointer-events-none">
            <For
                each=move || active_notifications.get()
                key=|n| n.id
                children=move |tracked_notif| {
                    let (title, message, bg_color, border_color, title_color, msg_color) = match tracked_notif
                        .event
                    {
                        NotificationEvent::UpdateAvailable => {
                            (
                                "Update Available".to_string(),
                                "A new version of the app is ready to install, check the settings."
                                    .to_string(),
                                "bg-blue-50 dark:bg-blue-950/30",
                                "border-blue-200 dark:border-blue-900",
                                "text-blue-800 dark:text-blue-300",
                                "text-blue-600 dark:text-blue-400",
                            )
                        }
                        NotificationEvent::UpdateDownloaded => {
                            (
                                "Update Downloaded".to_string(),
                                "The new version has been downloaded and is ready to install."
                                    .to_string(),
                                "bg-blue-50 dark:bg-blue-950/30",
                                "border-blue-200 dark:border-blue-900",
                                "text-blue-800 dark:text-blue-300",
                                "text-blue-600 dark:text-blue-400",
                            )
                        }
                        NotificationEvent::GenericNotification { title, message, level } => {
                            match level {
                                NotificationLevel::Info => {
                                    (
                                        title,
                                        message,
                                        "bg-green-50 dark:bg-green-950/30",
                                        "border-green-200 dark:border-green-900",
                                        "text-green-800 dark:text-green-300",
                                        "text-green-600 dark:text-green-400",
                                    )
                                }
                                NotificationLevel::Warning => {
                                    (
                                        title,
                                        message,
                                        "bg-yellow-50 dark:bg-yellow-950/30",
                                        "border-yellow-200 dark:border-yellow-900",
                                        "text-yellow-800 dark:text-yellow-300",
                                        "text-yellow-600 dark:text-yellow-400",
                                    )
                                }
                                NotificationLevel::Error => {
                                    (
                                        title,
                                        message,
                                        "bg-red-50 dark:bg-red-950/30",
                                        "border-red-200 dark:border-red-900",
                                        "text-red-800 dark:text-red-300",
                                        "text-red-600 dark:text-red-400",
                                    )
                                }
                            }
                        }
                    };

                    view! {
                        <div class=format!(
                            "pointer-events-auto flex flex-col border shadow-xl rounded-lg p-4 transition-all duration-300 animate-fade-in-up {} {}",
                            bg_color,
                            border_color,
                        )>
                            <p class=format!("text-sm font-semibold {}", title_color)>{title}</p>
                            <p class=format!("text-xs mt-0.5 {}", msg_color)>{message}</p>
                        </div>
                    }
                }
            />
        </div>
    }
}
