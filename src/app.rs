use crate::components::authentication::Authentication;
use crate::components::loading::Loading;
use crate::components::shader::BackgroundShader;
use shared::api::RestoreResponse;
use shared::sidebar::SidebarState;
use std::collections::HashMap;

use log::error;

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::account_data::{Breadcrumbs, ServerOrder};
use shared::user_profile::{PresenceInfo, UserProfile};
use wasm_bindgen::prelude::*;
use web_sys::HtmlImageElement;

use crate::components::{chat::Chat, sidebar::Sidebar};
use crate::hooks::use_tauri_event;
use crate::state::{AppState, MemberStore};
use crate::tauri_functions::{set_backend_room_id, set_focused_in_backend};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    pub fn invoke(cmd: &str, args: JsValue) -> js_sys::Promise;

    #[wasm_bindgen(js_namespace = ["__TAURI__", "opener"])]
    pub fn openUrl(url: &str) -> js_sys::Promise;
}

pub async fn call_tauri(cmd: &str, args: JsValue) -> Result<JsValue, JsValue> {
    wasm_bindgen_futures::JsFuture::from(invoke(cmd, args)).await
}

pub async fn call_tauri_no_args(cmd: &str) -> Result<JsValue, JsValue> {
    wasm_bindgen_futures::JsFuture::from(invoke(cmd, JsValue::NULL)).await
}

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum CurrentWindow {
    HomeserverDiscovery,
    Login,
    Home,
    Loading,
}

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state);

    let store = MemberStore::default();
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

    let profile_update = use_tauri_event::<(String, UserProfile)>("member_update");

    Effect::new(move |_| {
        let room_id = state.active_room_id.get();

        spawn_local(async move {
            if let Err(e) = set_backend_room_id(room_id).await {
                error!("Error setting backend room id: {:?}", e);
            };
        });
    });

    Effect::new(move |_| {
        if let Some((room_id, profile)) = profile_update.get() {
            store_for_profiles
                .get_profile(&room_id, &profile.user_id)
                .set(Some(profile.clone()));
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

    let presence_update = use_tauri_event::<HashMap<String, PresenceInfo>>("presence_update");

    Effect::new(move |_| {
        if let Some(updates) = presence_update.get() {
            for (user_id, presence) in updates.iter() {
                store_for_presences
                    .get_presence(user_id)
                    .set(presence.clone());
            }
        }
    });

    let sidebar_update_event: ReadSignal<Option<SidebarState>> = use_tauri_event("sidebar_update");

    Effect::new(move |_| {
        if let Some(mut new_state) = sidebar_update_event.get() {
            // new_state
            //     .dms
            //     .sort_by_key(|b| std::cmp::Reverse(b.last_ts().unwrap_or(0)));

            let current_order = state.server_order.get_untracked();

            let order_map: HashMap<&String, usize> = current_order
                .servers
                .iter()
                .enumerate()
                .map(|(index, id)| (id, index))
                .collect();

            new_state.servers.sort_by(|a, b| {
                let pos_a = order_map.get(&a.room_id).copied().unwrap_or(usize::MAX);
                let pos_b = order_map.get(&b.room_id).copied().unwrap_or(usize::MAX);

                if pos_a == usize::MAX && pos_b == usize::MAX {
                    let name_a = a.name.as_deref().unwrap_or("");
                    let name_b = b.name.as_deref().unwrap_or("");
                    return name_a.cmp(name_b);
                }

                pos_a.cmp(&pos_b)
            });

            let final_order: Vec<String> = new_state
                .servers
                .iter()
                .map(|s| s.room_id.clone())
                .collect();

            if final_order != current_order.servers {
                state.set_server_order(final_order);
            }

            state.sidebar_state.set(new_state);

            if state.current_window.get() == CurrentWindow::Loading {
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

                    state
                        .active_room_id
                        .set(breadcrumbs.recent_rooms.first().cloned());

                    if let Some(room_id) = breadcrumbs.recent_rooms.first().cloned() {
                        state.active_room_id.set(Some(room_id.clone()));

                        for (server, last_room) in breadcrumbs.clone().last_space_ids {
                            if last_room == room_id {
                                if server == "dms" {
                                    state.active_server_id.set(None);
                                } else {
                                    state.active_server_id.set(Some(server.clone()));
                                }
                                break;
                            }
                        }
                    }

                    state.breadcrums.set(breadcrumbs);
                }
                Err(err) => {
                    error!("Error fetching breadcrumbs: {:?}", err);
                }
            }

            match call_tauri_no_args("get_server_order").await {
                Ok(js_val) => {
                    let order: ServerOrder = serde_wasm_bindgen::from_value(js_val).unwrap();
                    state.server_order.set(order);
                }
                Err(err) => {
                    error!("Error fetching server order: {:?}", err);
                }
            };

            let _ = call_tauri_no_args("send_frontend").await;
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

    view! {
        <div
            class="bg-transparent flex h-screen overflow-hidden p-[var(--gap)] gap-[var(--gap)] relative"
            style=root_css_vars
        >
            <div data-tauri-drag-region class="absolute top-0 left-0 right-0 h-3 z-50"></div>
            <Sidebar />
            <Chat />
        </div>
    }
}
