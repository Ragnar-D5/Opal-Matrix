use std::collections::HashMap;

use leptos::leptos_dom::logging::console_error;
use leptos::task::spawn_local;
use leptos::{ev::SubmitEvent, prelude::*};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use web_sys::HtmlImageElement;

use crate::tauri::chat::ChatMessage;
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

async fn call_tauri(cmd: &str, args: JsValue) -> Result<JsValue, JsValue> {
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

#[derive(Clone, Debug, Copy)]
pub struct AppState {
    pub current_window: RwSignal<CurrentWindow>,
    pub login_name: RwSignal<String>,

    pub active_room_id: RwSignal<Option<String>>,
    pub active_server_id: RwSignal<Option<String>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_window: RwSignal::new(CurrentWindow::LoadingPage),
            login_name: RwSignal::new(String::new()),
            active_room_id: RwSignal::new(None),
            active_server_id: RwSignal::new(None),
        }
    }
}

#[component]
pub fn App() -> impl IntoView {
    let state = AppState::new();
    provide_context(state);

    // Invoke try_restore
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
                Err(_) => {}
            }
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
        <div class="login-wrapper">
            <h1>"Welcome to Tauri + Leptos"</h1>

            <div class="row">
                <a href="https://tauri.app" target="_blank">
                    <img src="public/tauri.svg" class="logo tauri" alt="Tauri logo"/>
                </a>
                <a href="https://docs.rs/leptos/" target="_blank">
                    <img src="public/leptos.svg" class="logo leptos" alt="Leptos logo"/>
                </a>
            </div>
            <p>"Click on the Tauri and Leptos logos to learn more."</p>

            <form style="display: flex; flex-direction: column;" on:submit=login>
                <input
                    id="username-input"
                    placeholder="Username"
                    on:input=move |ev| set_username.set(event_target_value(&ev))
                />
                <input
                    id="password-input"
                    placeholder="Password"
                    on:input=move |ev| set_password.set(event_target_value(&ev))
                    type="password"
                />
                <button type="submit">"Login"</button>
            </form>

            // Show errors if there are any
            <p style="color: red;">{ move || error_msg.get() }</p>
        </div>
    }
}

#[derive(Serialize)]
struct GetMessagesArgs {
    room_id: String,
    limit: usize,
}

#[component]
fn HomePage() -> impl IntoView {
    let state = expect_context::<AppState>();

    let (recovery_key, set_recovery_key) = signal(String::new());
    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());
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

    let send_recovery_key = move |ev: SubmitEvent| {
        ev.prevent_default();

        spawn_local(async move {
            let key = recovery_key.get_untracked();

            let args =
                serde_wasm_bindgen::to_value(&RecoveryKeyArgs { recovery_key: key }).unwrap();
            match call_tauri("set_recovery_key", args).await {
                Ok(_) => {}
                Err(err) => {
                    // Handle error, maybe show an error message
                }
            }
        });
    };

    // Get messages on room_id change
    Effect::new(move |_| {
        if let Some(room_id) = state.active_room_id.get() {
            console_error(&format!("Active room changed: {}", room_id));
            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&GetMessagesArgs {
                    room_id: room_id.clone(),
                    limit: 50,
                })
                .expect("Failed to create args");
                match call_tauri("get_messages", args).await {
                    Ok(js_val) => {
                        let new_messages: Vec<ChatMessage> =
                            serde_wasm_bindgen::from_value(js_val).unwrap();

                        set_messages.set(new_messages);
                    }
                    Err(err) => {
                        console_error(&format!(
                            "Failed to get messages: {}",
                            err.as_string().unwrap_or("Unknown error".to_string())
                        ));
                    }
                }
            });
        } else {
            set_messages.set(Vec::new());
        }
    });

    // preload once
    // {
    //     let bg_url = bg_url.clone();
    //     spawn_local(async move {
    //         let img = HtmlImageElement::new().unwrap();

    //         // onload -> mark loaded
    //         let onload = Closure::<dyn FnMut()>::new({
    //             let set_bg_loaded = set_bg_loaded;
    //             move || set_bg_loaded.set(true)
    //         });
    //         img.set_onload(Some(onload.as_ref().unchecked_ref()));
    //         onload.forget(); // keep callback alive

    //         // optional: onerror handling
    //         let onerror = Closure::<dyn FnMut()>::new(move || {
    //             // keep false, or set another error signal
    //         });
    //         img.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    //         onerror.forget();

    //         img.set_src(&bg_url);
    //     });
    // }

    let gap_size = "2".to_string();
    let padding = "2".to_string();

    let color_item_hover = "rgba(200, 200, 255, 0.05)";
    let color_item_selected = "rgba(255, 255, 255, 0.1)";
    let bg_color = "#1e1e2e";
    let floating_bg_color = "rgba(0, 0, 0, 0.4)";
    let pill_border_color = "rgba(255, 255, 255, 0.8)";
    let dim_text_color = "hsl(220, 15%, 40%)";
    let text_color = "hsl(220, 25%, 60%)";
    let bright_text_color = "hsl(220, 25%, 70%)";
    let tile_border_color = "rgba(30, 30, 30, 1)";

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
        <div class="bg-[var(--bg-color)] flex h-screen overflow-hidden p-3 gap-3 relative" style=root_css_vars>
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

    let send_home_server = move || {
        let current_value = text.get();

        spawn_local(async move {
            let args =
                serde_wasm_bindgen::to_value(&HomeServerArgs { url: current_value }).unwrap();

            match call_tauri("choose_home_server", args).await {
                Ok(url) => {
                    if url == *text.read() {
                        set_is_valid.set(true);
                    } else {
                        set_is_valid.set(false);
                    }
                    //if a server can be found here
                }
                Err(e) => {
                    let arr: js_sys::Array = e.into();
                    if arr.get(0) == *text.read() {
                        set_is_valid.set(false)
                    }
                    // if no server can be found here
                }
            }
        });
    };
    view! {
        <div style="display: flex; flex-direction: column; align-items: center; padding-top: 50px;">
            <input
                type="text"
                placeholder="example.org"
                on:input=move |ev| {
                    set_text.set(event_target_value(&ev));
                    send_home_server();
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
