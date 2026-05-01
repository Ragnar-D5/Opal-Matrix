use super::components::TextCircle;
use std::collections::{HashMap, HashSet};

use colorsys::Hsl;
use leptos::leptos_dom::logging::console_error;
use leptos::task::spawn_local;
use leptos::{ev::SubmitEvent, prelude::*};
use serde::{Deserialize, Serialize};
use shared::account_data::{AccountDataArgs, AccountDataPayload, Breadcrumbs, ServerOrder};
use shared::messages::UiMessage;
use wasm_bindgen::prelude::*;
use web_sys::HtmlImageElement;

use crate::hooks::use_tauri_event;
use crate::tauri::{chat::Chat, sidebar::Sidebar};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    fn invoke(cmd: &str, args: JsValue) -> js_sys::Promise;
}

#[derive(Deserialize, Debug, Clone)]
pub struct MatrixLoginResponse {
    pub user_id: String,
}

#[derive(Serialize, Deserialize)]
struct LoginArgs {
    matrix_url: String,
    username: String,
    password: String,
}

#[derive(Serialize, Deserialize)]
struct RecoveryKeyArgs {
    recovery_key: String,
}

pub async fn call_tauri(cmd: &str, args: JsValue) -> Result<JsValue, JsValue> {
    wasm_bindgen_futures::JsFuture::from(invoke(cmd, args)).await
}

pub async fn call_tauri_no_args(cmd: &str) -> Result<JsValue, JsValue> {
    wasm_bindgen_futures::JsFuture::from(invoke(cmd, JsValue::NULL)).await
}

#[derive(Clone, Debug, Copy)]
pub enum CurrentWindow {
    HomeserverDiscoveryPage,
    LoginPage,
    HomePage,
    LoadingPage,
}

#[derive(Deserialize, Clone, Debug)]
pub struct UserProfile {
    user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl UserProfile {
    pub fn render_icon(&self, size: usize) -> impl IntoView {
        let size_str = format!("{}px", size);

        let name = self.display_name.clone().unwrap_or(self.user_id.clone());

        match &self.avatar_url {
            Some(url) => view! {
                <img
                    class="rounded-full object-cover bg-transparent"
                    src=url
                    style:height=size_str.clone()
                    style:width=size_str
                    alt=name
                />
            }
            .into_any(),
            None => view! {
                <TextCircle
                    text=name
                    color_string=self.user_id.clone()
                    class="rounded-full"
                    style=format!("height: {}; width: {};", size_str, size_str)
                />
            }
            .into_any(),
        }
    }

    pub fn render_name(&self, font_size: usize) -> impl IntoView {
        let name = self.display_name.as_ref().unwrap_or(&self.user_id);
        let font_size_str = format!("{}px", font_size);
        let color = self.get_user_color().to_css_string();

        view! {
            <span
                style:font-size=font_size_str
                style:color=color
                class="font-semibold"
            >
                {name.clone()}
            </span>
        }
    }

    fn get_user_color(&self) -> Hsl {
        Self::get_color(self.user_id.clone())
    }

    pub fn get_color(string: String) -> Hsl {
        let mut hash: u32 = 0;
        for c in string.chars() {
            hash = (c as u32).wrapping_add(hash.wrapping_shl(5).wrapping_sub(hash));
        }

        let hue = hash % 360;

        Hsl::new(hue as f64, 90.0, 70.0, None)
    }
}

#[derive(Default, Clone)]
pub struct MemberStore {
    pub rooms: RwSignal<HashMap<String, HashMap<String, ArcRwSignal<UserProfile>>>>,
    pub fetching: RwSignal<HashSet<String>>,
}

#[derive(Serialize, Debug)]
struct GetMembersArgs {
    room_id: String,
}

impl MemberStore {
    pub fn get_profile(&self, room_id: &String, user_id: &String) -> ArcRwSignal<UserProfile> {
        let existing_signal = self.rooms.with_untracked(|rooms| {
            rooms
                .get(room_id)
                .and_then(|users| users.get(user_id))
                .cloned()
        });

        if let Some(sig) = existing_signal {
            return sig;
        }

        let new_signal = ArcRwSignal::new(UserProfile {
            user_id: user_id.clone(),
            display_name: None,
            avatar_url: None,
        });

        self.rooms.update(|rooms| {
            rooms
                .entry(room_id.clone())
                .or_default()
                .insert(user_id.clone(), new_signal.clone());
        });

        let is_fetching = self
            .fetching
            .with_untracked(|fetching| fetching.contains(room_id));

        if !is_fetching {
            self.fetching.update(|f| {
                f.insert(room_id.clone());
            });

            let store = self.clone();
            let rid = room_id.clone();

            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&GetMembersArgs {
                    room_id: rid.clone(),
                })
                .unwrap();

                if let Ok(js_val) = call_tauri("get_members", args).await {
                    let updates: HashMap<String, UserProfile> =
                        serde_wasm_bindgen::from_value(js_val).unwrap();

                    batch(move || {
                        store.rooms.update(|rooms| {
                            let room_entry = rooms.entry(rid.clone()).or_default();

                            for (user_id, profile) in updates.into_iter() {
                                let profile_signal = room_entry
                                    .entry(user_id.clone())
                                    .or_insert_with(|| ArcRwSignal::new(profile.clone()));

                                profile_signal.set(profile);
                            }
                        });
                        store.fetching.update(|f| {
                            f.remove(&rid);
                        });
                    });
                } else {
                    store.fetching.update(|f| {
                        f.remove(&rid);
                    });
                }
            });
        }

        new_signal
    }
}

#[derive(Clone, Debug, Copy)]
pub struct AppState {
    pub current_window: RwSignal<CurrentWindow>,
    pub login_name: RwSignal<String>,

    pub active_room_id: RwSignal<Option<String>>,
    pub active_server_id: RwSignal<Option<String>>,

    pub breadcrums: RwSignal<Breadcrumbs>,
    pub server_order: RwSignal<ServerOrder>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_window: RwSignal::new(CurrentWindow::LoadingPage),
            login_name: RwSignal::new(String::new()),
            active_room_id: RwSignal::new(None),
            active_server_id: RwSignal::new(None),
            breadcrums: RwSignal::new(Breadcrumbs::default()),
            server_order: RwSignal::new(ServerOrder::default()),
        }
    }

    pub fn set_active_room_id(&self, room_id: Option<String>) {
        self.active_room_id.set(room_id.clone());

        let Some(room_id) = room_id else {
            return;
        };

        let key = self.active_server_id.get().unwrap_or("dms".to_string());

        self.breadcrums.update(|bc| {
            bc.last_space_ids.insert(key, room_id.clone());
        });

        self.append_room_id(room_id.clone());
        self.save_breadcrumbs();
    }

    pub fn set_active_server_id(&self, server_id: Option<String>) {
        self.active_server_id.set(server_id.clone());

        let key = server_id.clone().unwrap_or("dms".to_string());

        if let Some(room_id) = self.breadcrums.get().last_space_ids.get(&key).cloned() {
            self.active_room_id.set(Some(room_id.clone()));
            self.append_room_id(room_id);

            self.save_breadcrumbs();
        }
    }

    fn append_room_id(&self, room_id: String) {
        self.breadcrums.update(|bc| {
            bc.recent_rooms.retain(|id| id != &room_id);
            bc.recent_rooms.insert(0, room_id);

            if bc.recent_rooms.len() > 10 {
                bc.recent_rooms.pop();
            }
        });
    }

    fn save_breadcrumbs(&self) {
        let breadcrumbs = self.breadcrums.get();
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&AccountDataArgs {
                payload: AccountDataPayload::Breadcrumbs(breadcrumbs),
            })
            .expect("Failed to serialize breadcrumbs");
            if let Err(err) = call_tauri("set_account_data", args).await {
                console_error(&format!("Error saving breadcrumbs: {:?}", err));
            }
        });
    }

    pub fn set_server_order(&self, servers: Vec<String>) {
        self.server_order.set(ServerOrder { servers });
        self.save_server_order();
    }

    fn save_server_order(&self) {
        let order = self.server_order.get_untracked();
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&AccountDataArgs {
                payload: AccountDataPayload::ServerOrder(order),
            })
            .expect("Failed to serialize server order");
            if let Err(err) = call_tauri("set_account_data", args).await {
                console_error(&format!("Error saving server order: {:?}", err));
            }
        });
    }
}

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state);

    let store = MemberStore::default();
    provide_context(store);

    let profile_update =
        use_tauri_event::<HashMap<String, HashMap<String, UserProfile>>>("member_update");

    Effect::new(move |_| {
        spawn_local(async move {
            match call_tauri_no_args("try_restore").await {
                Ok(js_val) => {
                    let response_option: Option<MatrixLoginResponse> =
                        serde_wasm_bindgen::from_value(js_val).unwrap();

                    if let Some(response) = response_option {
                        state.login_name.set(response.user_id);
                        state.current_window.set(CurrentWindow::HomePage);
                    } else {
                        state
                            .current_window
                            .set(CurrentWindow::HomeserverDiscoveryPage);
                    }
                }
                Err(_) => {
                    console_error("Error during restore, showing home server discovery");
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
                    console_error(&format!("Error fetching breadcrumbs: {:?}", err));
                }
            }

            match call_tauri_no_args("get_server_order").await {
                Ok(js_val) => {
                    let order: ServerOrder = serde_wasm_bindgen::from_value(js_val).unwrap();
                    state.server_order.set(order);
                }
                Err(err) => {
                    console_error(&format!("Error fetching server order: {:?}", err));
                }
            };

            let _ = call_tauri_no_args("send_frontend").await;
        });
    });

    view! {
        {move || match state.current_window.get() {
            CurrentWindow::HomeserverDiscoveryPage => view! {
                <HomeserverDiscoveryPage/>
            }.into_any(),
            CurrentWindow::LoginPage => view! {
                <LoginPage/>
            }.into_any(),

            CurrentWindow::HomePage => view! {
                <HomePage/>
            }.into_any(),

            CurrentWindow::LoadingPage => view! {
                <div class="loading">
                    <p>"Loading..."</p>
                </div>
            }.into_any(),
        }}
    }
}

#[component]
fn LoginPage() -> impl IntoView {
    let state = expect_context::<AppState>();

    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (error_msg, set_error_msg) = signal(String::new());

    let login = move |ev: SubmitEvent| {
        ev.prevent_default();
        spawn_local(async move {
            let username = username.get_untracked();
            let password = password.get_untracked();

            if username.is_empty() || password.is_empty() {
                return;
            }

            let args = serde_wasm_bindgen::to_value(&LoginArgs {
                username: username,
                password: password,
                matrix_url: "https://matrix.erik-is.gay".to_string(),
            })
            .unwrap();

            match call_tauri("login", args).await {
                Ok(js_val) => {
                    let response: MatrixLoginResponse =
                        serde_wasm_bindgen::from_value(js_val).unwrap();

                    state.login_name.set(response.user_id);
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
                    on:input=move |ev| set_username.set(event_target_value(&ev))
                />
                <input
                    id="password-input"
                    placeholder="Password"
                    class="p-2.5 text-xl rounded-lg select-none"
                    on:input=move |ev| set_password.set(event_target_value(&ev))
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
            <p class="text-red-300">{ move || error_msg.get() }</p>
        </div>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    let state = expect_context::<AppState>();

    let (recovery_key, set_recovery_key) = signal(String::new());
    let (messages, set_messages) = signal(Vec::<UiMessage>::new());
    let (bg_loaded, set_bg_loaded) = signal(false);
    let bg_url = "https://i.imgur.com/t9plvkd.pn".to_string();

    let bg_url_clone = bg_url.clone();
    Effect::new(move |_| {
        let img = HtmlImageElement::new().unwrap();
        let onload = Closure::<dyn FnMut()>::new(move || set_bg_loaded.set(true));
        img.set_onload(Some(onload.as_ref().unchecked_ref()));
        onload.forget();
        img.set_src(&bg_url_clone);
    });

    let send_recovery_key = move |ev: SubmitEvent| {
        ev.prevent_default();

        spawn_local(async move {
            let key = recovery_key.get_untracked();

            let args =
                serde_wasm_bindgen::to_value(&RecoveryKeyArgs { recovery_key: key }).unwrap();
            match call_tauri("set_recovery_key", args).await {
                Ok(_) => {}
                Err(err) => {
                    console_error(&format!("Error setting recovery key: {:?}", err));
                }
            }
        });
    };

    let color_item_hover = "rgba(200, 200, 255, 0.05)";
    let color_item_selected = "rgba(255, 255, 255, 0.1)";
    let bg_color = "#1e1e2e";
    let floating_bg_color = "rgba(0, 0, 0, 0.4)";
    let pill_border_color = "rgba(255, 255, 255, 0.8)";
    let dim_text_color = "hsl(220, 15%, 40%)";
    let text_color = "hsl(220, 25%, 60%)";
    let bright_text_color = "hsl(220, 25%, 70%)";
    // let tile_border_color = "rgba(30, 30, 30, 1)";
    let tile_border_color = "rgba(255, 255, 255, 0.3)";
    let muted_text_color = "hsl(220, 15%, 25%)";
    let gap = "5px";
    let mention_color = "rgb(255, 100, 100)";

    let root_css_vars = move || {
        let base = format!(
            "--color-item-hover: {color_item_hover};
            --color-item-selected: {color_item_selected};
            --bg-color: {bg_color};
            --floating-bg-color: {floating_bg_color};
            --pill-border-color: {pill_border_color};
            --dim-text-color: {dim_text_color};
            --text-color: {text_color};
            --bright-text-color: {bright_text_color};
            --tile-border-color: {tile_border_color};
            --muted-text-color: {muted_text_color};
            --gap: {gap};
            --mention-color: {mention_color};
            background-color: {bg_color};",
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
        <div class="bg-[var(--bg-color)] flex h-screen overflow-hidden p-[var(--gap)] gap-[var(--gap)] relative" style=root_css_vars>
        // <h2>"Login Successful!"</h2>
        // <p>"Welcome, " <strong>{user_id}</strong></p>

        // <form on:submit=send_recovery_key>
        //     <input placeholder="Recovery Key" on:input=move |ev| set_recovery_key.set(event_target_value(&ev)) />
        //     <button type="submit">"Set Recovery Key"</button>
        // </form>

            <div data-tauri-drag-region class="absolute top-0 left-0 right-0 h-3 z-50"></div>
            <Sidebar />
            <Chat messages=messages set_messages=set_messages />
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

            match call_tauri("try_home_server", args).await {
                Ok(url) => {
                    if url == *text.read_untracked() {
                        set_is_valid.set(true);
                    } else {
                        set_is_valid.set(false);
                    }
                    //if a server can be found here
                }
                Err(e) => {
                    let arr: js_sys::Array = e.into();
                    if arr.get(0) == *text.read_untracked() {
                        set_is_valid.set(false)
                    }
                    // if no server can be found here
                }
            }
        });
    };

    let choose_home_server = move || {
        let chosen_server = text.get_untracked();

        spawn_local(async move {
            let args =
                serde_wasm_bindgen::to_value(&HomeServerArgs { url: chosen_server }).unwrap();

            // TODO: refactor code be less duplicate, see discovery.rs for reference
            call_tauri("choose_home_server", args).await.unwrap();
        });
    };

    view! {
        <div style="display: flex; flex-direction: column; align-items: center; padding-top: 50px;">
            <input
                type="text"
                placeholder="example.org"
                on:input=move |ev| {
                    set_text.set(event_target_value(&ev));
                    try_home_server();
                }
                prop:value=text
                style="padding: 10px; font-size: 1.2rem; border-radius: 8px;"
            />

            // The button only renders when is_valid is true
            <Show
                when=move || is_valid.get()
                fallback=|| view! { <p style="color: gray;">"Checking server..."</p> }
            >
                <button
                    on:click=move |_| {
                        choose_home_server();
                        state.current_window.set(CurrentWindow::LoginPage);
                    }
                    style="margin-top: 20px; padding: 10px 20px; background-color: #007bff; color: white; border: none; border-radius: 5px; cursor: pointer;"
                >
                    "Login Page"
                </button>
            </Show>
        </div>
    }
}
