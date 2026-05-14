use crate::components::loading::Loading;
use crate::components::shader::BackgroundShader;
use shared::sidebar::SidebarState;
use std::collections::HashMap;

use log::error;

use leptos::task::spawn_local;
use leptos::{ev::SubmitEvent, prelude::*};
use serde::{Deserialize, Serialize};
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

#[derive(Deserialize, Debug, Clone)]
pub struct MatrixLoginResponse {
    pub user_id: String,
}

#[derive(Serialize, Deserialize)]
struct LoginArgs {
    username: String,
    password: String,
    recovery_key: String,
}

pub async fn call_tauri(cmd: &str, args: JsValue) -> Result<JsValue, JsValue> {
    wasm_bindgen_futures::JsFuture::from(invoke(cmd, args)).await
}

pub async fn call_tauri_no_args(cmd: &str) -> Result<JsValue, JsValue> {
    wasm_bindgen_futures::JsFuture::from(invoke(cmd, JsValue::NULL)).await
}

#[derive(Clone, Debug, Copy, PartialEq)]
pub enum CurrentWindow {
    HomeserverDiscoveryPage,
    LoginPage,
    HomePage,
    LoadingPage,
}

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state);

    let store = MemberStore::default();
    let store_for_profiles = store.clone();
    let store_for_presences = store.clone();

    provide_context(store);

    Effect::new(move |_| {
        if state.current_window.get() != CurrentWindow::LoadingPage
            && state.loading_time.get() == 0.0
        {
            if let Some(perf) = web_sys::window().and_then(|w| w.performance()) {
                state.loading_time.set(perf.now() / 1000.0);
            }
        }
    });

    let profile_update =
        use_tauri_event::<HashMap<String, HashMap<String, UserProfile>>>("member_update");

    Effect::new(move |_| {
        let room_id = state.active_room_id.get();

        spawn_local(async move {
            if let Err(e) = set_backend_room_id(room_id).await {
                error!("Error setting backend room id: {:?}", e);
            };
        });
    });

    Effect::new(move |_| {
        if let Some(updates) = profile_update.get() {
            for (room_id, users) in updates.iter() {
                for (user_id, profile) in users.iter() {
                    store_for_profiles
                        .get_profile(room_id, user_id)
                        .set(Some(profile.clone()));
                }
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
            new_state
                .dms
                .sort_by(|a, b| b.last_ts().unwrap_or(0).cmp(&a.last_ts().unwrap_or(0)));

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

            if state.current_window.get() == CurrentWindow::LoadingPage {
                state.current_window.set(CurrentWindow::HomePage);
            }
        }
    });

    Effect::new(move |_| {
        spawn_local(async move {
            match call_tauri_no_args("try_restore").await {
                Ok(js_val) => {
                    let response_option: Option<MatrixLoginResponse> =
                        serde_wasm_bindgen::from_value(js_val).unwrap();

                    if let Some(response) = response_option {
                        state.user_id.set(response.user_id);
                    } else {
                        state
                            .current_window
                            .set(CurrentWindow::HomeserverDiscoveryPage);
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
            CurrentWindow::HomeserverDiscoveryPage => {
                view! { <HomeserverDiscoveryPage /> }.into_any()
            }
            CurrentWindow::LoginPage => view! { <LoginPage /> }.into_any(),
            CurrentWindow::HomePage => view! { <HomePage /> }.into_any(),
            CurrentWindow::LoadingPage => Loading().into_any(),
        }}
    }
}

#[component]
fn LoginPage() -> impl IntoView {
    let state = expect_context::<AppState>();

    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (recovery_key, set_recovery_key) = signal(String::new());
    let (error_msg, set_error_msg) = signal(String::new());

    let username_ref = NodeRef::<leptos::html::Input>::new();
    Effect::new(move || {
        if let Some(el) = username_ref.get() {
            let _ = el.focus();
        }
    });

    let login = move |ev: SubmitEvent| {
        ev.prevent_default();
        spawn_local(async move {
            let username = username.get_untracked();
            let password = password.get_untracked();
            let recovery_key = recovery_key.get_untracked();

            if username.is_empty() || password.is_empty() || recovery_key.is_empty() {
                return;
            }

            let args = serde_wasm_bindgen::to_value(&LoginArgs {
                username: username,
                password: password,
                recovery_key: recovery_key,
            })
            .unwrap();

            match call_tauri("login", args).await {
                Ok(js_val) => {
                    let response: MatrixLoginResponse =
                        serde_wasm_bindgen::from_value(js_val).unwrap();

                    state.user_id.set(response.user_id);
                    state.current_window.set(CurrentWindow::HomePage);
                }
                Err(err) => {
                    let err_str = err
                        .as_string()
                        .unwrap_or_else(|| "Unknown error".to_string());

                    // Display the error locally
                    set_error_msg.set(err_str);
                }
            };

            // Do some loading animation
        });
    };

    view! {
        <div class="flex flex-col items-center pt-[50px]">

            <form class="flex flex-col gap-4" on:submit=login>
                <input
                    id="username-input"
                    placeholder="Username"
                    class="p-2.5 text-xl rounded-lg select-none"
                    node_ref=username_ref
                    on:input=move |ev| set_username.set(event_target_value(&ev))
                />
                <input
                    id="password-input"
                    placeholder="Password"
                    class="p-2.5 text-xl rounded-lg select-none"
                    on:input=move |ev| set_password.set(event_target_value(&ev))
                    type="password"
                />
                <input
                    id="recovery-key-input"
                    placeholder="Recovery Key"
                    class="p-2.5 text-xl rounded-lg select-none"
                    on:input=move |ev| set_recovery_key.set(event_target_value(&ev))
                    type="password"
                />
                <button
                    type="submit"
                    class="mt-5 px-5 py-2.5 bg-blue-500 text-white rounded-md border-none cursor-pointer select-none"
                >
                    "Login"
                </button>
            </form>

            // Show errors if there are any
            <p class="text-red-300">{move || error_msg.get()}</p>
        </div>
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
        let base = format!(
            "cursor: default;

            line-height: 22px",
        );

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

#[derive(Serialize, Deserialize)]
struct HomeServerArgs {
    url: String,
}

#[component]
pub fn HomeserverDiscoveryPage() -> impl IntoView {
    let state = expect_context::<AppState>();
    let (text, set_text) = signal(String::new());
    let (is_valid, set_is_valid) = signal(false);

    let try_home_server = move || {
        let current_value = text.get();

        spawn_local(async move {
            let args =
                serde_wasm_bindgen::to_value(&HomeServerArgs { url: current_value }).unwrap();

            match call_tauri("choose_home_server", args).await {
                Ok(url) => {
                    if url == *text.read_untracked() {
                        set_is_valid.set(true);
                    } else {
                        set_is_valid.set(false);
                    }
                    //if a server can be found here
                }
                Err(_) => {}
            }
        });
    };

    let choose_home_server = async move || {
        let chosen_server = text.get_untracked();

        let args = serde_wasm_bindgen::to_value(&HomeServerArgs { url: chosen_server }).unwrap();

        // TODO: refactor code be less duplicate, see discovery.rs for reference
        call_tauri("choose_home_server", args).await.unwrap();
    };

    view! {
        <div class="flex flex-col items-center pt-[50px]">
            <input
                type="text"
                placeholder="example.org"
                class="p-2.5 text-xl rounded-lg select-none"
                autofocus
                on:input=move |ev| {
                    set_text.set(event_target_value(&ev));
                    set_is_valid.set(false);
                    try_home_server();
                }
                prop:value=text
            />

            // The button only renders when is_valid is true
            <Show
                when=move || is_valid.get()
                fallback=|| view! { <p class="text-gray-600 select-none">"Checking server..."</p> }
            >
                <button
                    class="mt-5 px-5 py-2.5 bg-blue-500 text-white rounded-md border-none cursor-pointer select-none"
                    on:click=move |_| {
                        spawn_local(async move {
                            choose_home_server().await;
                            state.current_window.set(CurrentWindow::LoginPage);
                        })
                    }
                >
                    "Login Page"
                </button>
            </Show>
        </div>
    }
}
