use leptos::task::spawn_local;
use leptos::{ev::SubmitEvent, prelude::*};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

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

async fn call_tauri(cmd: &str, args: JsValue) -> Result<JsValue, JsValue> {
    wasm_bindgen_futures::JsFuture::from(invoke(cmd, args)).await
}

#[component]
pub fn App() -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());

    let (login_name, set_login_name) = signal(String::new());

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

            match call_tauri("matrix_login", args).await {
                Ok(js_val) => {
                    let response: MatrixLoginResponse =
                        serde_wasm_bindgen::from_value(js_val).unwrap();

                    set_login_name.set(response.user_id)
                }
                Err(err) => {
                    let err_str = err
                        .as_string()
                        .unwrap_or_else(|| "Unknown error".to_string());

                    set_login_name.set(err_str)
                }
            };
        });
    };

    view! {
        <main class="container">
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
            <p>{ move || login_name.get() }</p>
        </main>
    }
}
