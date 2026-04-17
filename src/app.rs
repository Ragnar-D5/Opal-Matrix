use leptos::leptos_dom::logging::console_error;
use leptos::task::spawn_local;
use leptos::{ev::SubmitEvent, prelude::*};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::tauri::sidebar::Sidebar;
use crate::theming::ThemeProvider;

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

#[derive(Clone)]
enum CurrentWindow {
    HomeserverDiscoveryPage,
    LoginPage,
    HomePage,
    LoadingPage,
}

#[component]
pub fn App() -> impl IntoView {
    // Global state that both overlays might need to interact with
    let (app_state, set_app_state) = signal(CurrentWindow::LoadingPage);
    let (login_name, set_login_name) = signal(String::new());

    // Invoke try_restore
    spawn_local(async move {
        match call_tauri_no_args("try_restore").await {
            Ok(js_val) => {
                let response_option: Option<MatrixLoginResponse> =
                    serde_wasm_bindgen::from_value(js_val).unwrap();

                if let Some(response) = response_option {
                    set_login_name.set(response.user_id);
                    set_app_state.set(CurrentWindow::HomePage);
                } else {
                    set_app_state.set(CurrentWindow::HomeserverDiscoveryPage);
                }
            }
            Err(_) => {}
        }
    });

    view! {
        {move || match app_state.get() {
            CurrentWindow::HomeserverDiscoveryPage => view! {
                <HomeserverDiscoveryPage
                    set_app_state=set_app_state
                />
            }.into_any(),
            CurrentWindow::LoginPage => view! {
                <LoginPage
                    set_app_state=set_app_state
                    set_login_name=set_login_name
                />
            }.into_any(),

            CurrentWindow::HomePage => view! {
                <HomePage user_id=login_name.get() />
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
fn LoginPage(
    set_app_state: WriteSignal<CurrentWindow>,
    set_login_name: WriteSignal<String>,
) -> impl IntoView {
    // Local state: Only this component needs to know about what's typed in the boxes
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());

    // We can also make a local error signal instead of reusing login_name for errors
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

                    set_login_name.set(response.user_id);
                    set_app_state.set(CurrentWindow::HomePage);
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

#[component]
fn HomePage(user_id: String) -> impl IntoView {
    let (recovery_key, set_recovery_key) = signal(String::new());

    let (active_room_id, set_active_room_id) = signal(None::<String>);
    let (active_server_id, set_active_server_id) = signal(None::<String>);

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

    view! {
        <div class="bg flex h-screen">
        // <h2>"Login Successful!"</h2>
        // <p>"Welcome, " <strong>{user_id}</strong></p>

        // <form on:submit=send_recovery_key>
        //     <input placeholder="Recovery Key" on:input=move |ev| set_recovery_key.set(event_target_value(&ev)) />
        //     <button type="submit">"Set Recovery Key"</button>
        // </form>

            <Sidebar active_room_id=active_room_id
                        set_active_room_id=set_active_room_id
                        active_server_id=active_server_id
                        set_active_server_id=set_active_server_id />
        </div>
    }
}

#[derive(Serialize, Deserialize)]
struct HomeServerArgs {
    url: String,
}

#[component]
pub fn HomeserverDiscoveryPage(set_app_state: WriteSignal<CurrentWindow>) -> impl IntoView {
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
                        set_app_state.set(CurrentWindow::LoginPage);
                    }
                    style="margin-top: 20px; padding: 10px 20px; background-color: #007bff; color: white; border: none; border-radius: 5px; cursor: pointer;"
                >
                    "Login Page"
                </button>
            </Show>
        </div>
    }
}
