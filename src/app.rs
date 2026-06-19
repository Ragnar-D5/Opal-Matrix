use crate::components::authentication::Authentication;
use crate::components::loading::Loading;
use crate::components::previews::ImageLightbox;
use crate::components::shader::BackgroundShader;
use chrono::{DateTime, Local};
use shared::api::RestoreResponse;
use shared::sidebar::{NotificationCounts, SidebarState, UserDevice};
use std::collections::HashMap;

use log::error;

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::account_data::{Breadcrumbs, ServerOrder};
use shared::profile::{MemberProfile, PresenceInfo};
use wasm_bindgen::prelude::*;
use web_sys::HtmlImageElement;

use crate::components::{
    chat::Chat,
    overlays::emoji_picker::{EmojiPickerPortal, EmojiPickerState},
    overlays::gif_picker::{GifPickerPortal, GifPickerState},
    overlays::profile_card::{ProfileCardPortal, ProfileCardState},
    sidebar::Sidebar,
};
use crate::hooks::use_tauri_event;
use crate::state::{AppState, ProfileStore};
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

    let profile_updates: ReadSignal<Option<HashMap<String, Vec<MemberProfile>>>> =
        use_tauri_event("member_update");

    Effect::new(move |_| {
        let room_id = state.active_room_id();

        spawn_local(async move {
            if let Err(e) = set_backend_room_id(room_id).await {
                error!("Error setting backend room id: {:?}", e);
            };
        });
    });

    Effect::new(move |_| {
        let Some(update) = profile_updates.get() else {
            return;
        };

        for (room_id, profiles) in update {
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

    let presence_update: ReadSignal<Option<HashMap<String, PresenceInfo>>> =
        use_tauri_event("presence_update");

    Effect::new(move |_| {
        if let Some(updates) = presence_update.get() {
            for (user_id, presence) in updates.iter() {
                store_for_presences
                    .get_presence(user_id)
                    .set(presence.clone());
            }
        }
    });

    let notification_counts_update: ReadSignal<Option<HashMap<String, NotificationCounts>>> =
        use_tauri_event("notification_counts_update");

    Effect::new(move |_| {
        if let Some(updates) = notification_counts_update.get() {
            state
                .notification_counts
                .update(|counts| counts.extend(updates));
        }
    });

    let call_member_update: ReadSignal<Option<HashMap<String, Vec<UserDevice>>>> =
        use_tauri_event("call_member_update");

    Effect::new(move |_| {
        if let Some(update) = call_member_update.get() {
            state.update_call_members(update);
        }
    });

    let typing_update: ReadSignal<Option<(String, Vec<String>)>> = use_tauri_event("typing_update");

    Effect::new(move |_| {
        if let Some((room_id, user_ids)) = typing_update.get() {
            state.update_typing_users(&room_id, user_ids);
        }
    });

    let sidebar_update_event: ReadSignal<Option<SidebarState>> = use_tauri_event("sidebar_update");

    Effect::new(move |_| {
        if let Some(mut new_state) = sidebar_update_event.get() {
            let current_order = state.server_order.get_untracked();

            let order_map: HashMap<&String, usize> = current_order
                .servers
                .iter()
                .enumerate()
                .map(|(index, id)| (id, index))
                .collect();

            new_state.top_level_servers.sort_by(|a, b| {
                let pos_a = order_map.get(a).copied().unwrap_or(usize::MAX);
                let pos_b = order_map.get(b).copied().unwrap_or(usize::MAX);

                pos_a.cmp(&pos_b)
            });

            let final_order = new_state.top_level_servers.clone();

            if final_order != current_order.servers {
                state.server_order.set(ServerOrder {
                    servers: final_order,
                });
            }

            state.sidebar_state.set(new_state);
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
                        let server_id = state.find_server_id_for_room(room_id).or_else(|| {
                            breadcrumbs
                                .last_space_ids
                                .iter()
                                .find(|(_, last_room)| last_room.as_str() == room_id.as_str())
                                .filter(|(server, _)| server.as_str() != "dms")
                                .map(|(server, _)| server.clone())
                        });
                        state.active_server_id.set(server_id);
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
    #[setting("Show url perviews default", true)]
    pub url_previews_default: bool,
}
