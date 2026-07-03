use crate::components::authentication::Authentication;
use crate::components::loading::Loading;
use crate::components::previews::ImageLightbox;
use crate::components::shader::BackgroundShader;
use chrono::{DateTime, Local};
use shared::api::events::{
    CallMemberUpdate, NotificationEvent, NotificationLevel, NotificationUpdate, PresenceUpdate,
    ProfileUpdates, TypingUpdate,
};
use shared::api::{AudioDeviceInfos, RestoreResponse};
use shared::sidebar::{DmList, RoomMapUpdate, ServerList, SingleList};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use log::error;

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::account_data::{Breadcrumbs, ServerOrder};
use wasm_bindgen::prelude::*;
use web_sys::HtmlImageElement;

use crate::components::{
    chat::Chat,
    overlays::emoji_picker::{EmojiPickerPortal, EmojiPickerState},
    overlays::gif_picker::{GifPickerPortal, GifPickerState},
    overlays::profile_card::{ProfileCardPortal, ProfileCardState},
    sidebar::Sidebar,
    SystemButtons,
};
use crate::hooks::{setup_update_effect, use_tauri_event};
use crate::state::{AppState, CurrentSection, ProfileStore};
use crate::tauri_functions::{get_server_order, set_backend_room_id, set_focused_in_backend};

use macros::matrix_settings;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    pub fn invoke(cmd: &str, args: JsValue) -> js_sys::Promise;

    #[wasm_bindgen(js_namespace = ["__TAURI__", "core"], js_name = convertFileSrc)]
    pub fn convertFileSrc(path: &str) -> String;

    #[wasm_bindgen(js_namespace = ["__TAURI__", "opener"])]
    pub fn openUrl(url: &str) -> js_sys::Promise;
}

pub async fn call_tauri(cmd: &str, args: JsValue) -> Result<JsValue, JsValue> {
    wasm_bindgen_futures::JsFuture::from(invoke(cmd, args)).await
}

pub async fn call_tauri_no_args(cmd: &str) -> Result<JsValue, JsValue> {
    wasm_bindgen_futures::JsFuture::from(invoke(cmd, JsValue::NULL)).await
}

#[derive(Clone, Debug, Copy, PartialEq, Default)]
pub enum CurrentWindow {
    HomeserverDiscovery,
    Login,
    Home,
    #[default]
    Loading,
}

pub fn format_date(date: DateTime<Local>) -> String {
    match (date.date_naive() - Local::now().date_naive()).num_days() {
        0 => date.format("Today, %H:%M").to_string(),
        -1 => date.format("Yesterday, %H:%M").to_string(),
        -6..-1 => date.format("%a %d/%m/%Y, %H:%M").to_string(),
        _ => date.format("%d/%m/%Y, %H:%M").to_string(),
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

#[component]
pub fn App() -> impl IntoView {
    spawn_local(async move {
        let _ = call_tauri_no_args("get_devices").await;
    });
    let state = AppState::default();
    provide_context(state);

    let settings = Settings::default();
    settings.setup_backend_hook();

    provide_context(settings);

    let store = ProfileStore::default();
    let store_for_profiles = store.clone();
    let store_for_presences = store.clone();

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
                store_for_profiles
                    .get_member_profile(&room_id, &profile.profile.user_id)
                    .set(profile.clone());
            }
        }
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
            store_for_presences
                .get_presence(user_id)
                .set(presence.clone());
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

    Effect::new(move |_| {
        if let Some(mut new_state) = server_list_event.get() {
            let current_order = state.server_order.get_untracked();

            let order_map: HashMap<&String, usize> = current_order
                .servers
                .iter()
                .enumerate()
                .map(|(index, id)| (id, index))
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

            if state.current_window.get_untracked() == CurrentWindow::Loading {
                state.current_window.set(CurrentWindow::Home);
            }
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
                            state.user_id.set(user_id);
                            state.current_window.set(CurrentWindow::Home);
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

            match call_tauri_no_args("get_breadcrumbs").await {
                Ok(js_val) => {
                    let breadcrumbs: Breadcrumbs = serde_wasm_bindgen::from_value(js_val).unwrap();

                    if let Some(room_id) = breadcrumbs.recent_rooms.first() {
                        let section = if state.dm_list.get_untracked().0.contains(room_id) {
                            CurrentSection::Dms
                        } else if state.single_room_list.get_untracked().0.contains(room_id) {
                            CurrentSection::Single
                        } else {
                            match state.find_server_id_for_room(room_id) {
                                Some(server_id) => CurrentSection::Server(server_id),
                                None => CurrentSection::default(),
                            }
                        };
                        state.active_section.set(section);
                    }

                    state.breadcrums.set(breadcrumbs.clone());

                    state.set_active_room_with_id(breadcrumbs.recent_rooms.first().cloned());
                }
                Err(err) => {
                    error!("Error fetching breadcrumbs: {:?}", err);
                }
            }

            match get_server_order().await {
                Ok(order) => {
                    state.server_order.set(order);
                    state.apply_server_order();
                }
                Err(err) => {
                    error!("Error fetching server order: {:?}", err);
                }
            };

            if let Err(e) = settings.get_all().await {
                log::error!("Failed to get settings from backend: {:?}", e);
            };

            state.data_initialized.set(true);
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

    view! {
        <div
            class="bg-transparent flex h-screen overflow-hidden p-[var(--gap)] gap-[var(--gap)] relative"
            style=root_css_vars
        >
            <div data-tauri-drag-region class="absolute top-0 left-0 right-0 h-3 z-50"></div>
            <div class="absolute top-[var(--gap)] right-[var(--gap)] z-9999 flex items-center h-(--header-height) px-(--system-button-padding)">
                <SystemButtons />
            </div>
            <Sidebar />
            <Chat />
            <ImageLightbox />
            <EmojiPickerPortal />
            <GifPickerPortal />
            <ProfileCardPortal />
        </div>
    }
}

#[matrix_settings]
pub struct Settings {
    #[setting("Scaling", false, default = 1.0)]
    pub scaling: f64,
    #[setting("Url Previews per room", true)]
    pub url_previews: HashMap<String, bool>,
    #[setting("Show url perviews default", false)]
    pub url_previews_default: bool,
    #[setting("Show image border", false, default = true)]
    pub show_image_border: bool,
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
                    let (title, message, bg_color, border_color) = match tracked_notif.event {
                        NotificationEvent::UpdateAvailable => {
                            (
                                "Update Available".to_string(),
                                "A new version of the app is ready to install.".to_string(),
                                "bg-blue-50 dark:bg-blue-950/30",
                                "border-blue-200 dark:border-blue-900",
                            )
                        }
                        NotificationEvent::GenericNotification { title, message, level } => {
                            let (bg, border) = match level {
                                NotificationLevel::Info => {
                                    (
                                        "bg-green-50 dark:bg-blue-950/30",
                                        "border-green-200 dark:border-green-900",
                                    )
                                }
                                NotificationLevel::Warning => {
                                    (
                                        "bg-yellow-50 dark:bg-yellow-950/30",
                                        "border-yellow-200 dark:border-yellow-900",
                                    )
                                }
                                NotificationLevel::Error => {
                                    (
                                        "bg-red-50 dark:bg-red-950/30",
                                        "border-red-200 dark:border-red-900",
                                    )
                                }
                            };
                            (title, message, bg, border)
                        }
                    };

                    view! {
                        <div class=format!(
                            "pointer-events-auto flex flex-col border shadow-xl rounded-lg p-4 transition-all duration-300 animate-fade-in-up text-gray-900 dark:text-gray-100 {} {}",
                            bg_color,
                            border_color,
                        )>
                            <p class="text-sm font-semibold">{title}</p>
                            <p class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">{message}</p>
                        </div>
                    }
                }
            />
        </div>
    }
}
