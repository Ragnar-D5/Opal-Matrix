use crate::{
    app::{CurrentWindow, call_tauri, call_tauri_no_args},
    components::{
        SingleFloatingTile, SystemButtonsInTile, input::move_caret_to_end, settings::Settings,
    },
    state::{AppState, MainView},
    tauri_functions::get_server_order,
};
use leptos::{html::Input, prelude::*, task::spawn_local};
use serde_json::json;
use shared::{account_data::Breadcrumbs, api::errors::LoginError};
use web_sys::{HtmlInputElement, SubmitEvent};

#[component]
fn ArgSpan(text: impl ToString) -> impl IntoView {
    view! { <span class="text-dim text-xl self-start pb-2 select-none">{text.to_string()}</span> }
}

#[derive(Clone)]
enum LoginInputStatus {
    Loading,
    Valid,
    UsernameEmpty,
    PasswordEmpty,
    RecoveryKeyEmpty,
    RecoveryKeyInvalid,
    Error(String),
}

impl LoginInputStatus {
    fn render(self) -> impl IntoView {
        match self {
            LoginInputStatus::Valid => {
                view! { <p class="text-green-600 select-none">"All inputs look good"</p> }
                    .into_any()
            }
            LoginInputStatus::UsernameEmpty => {
                view! { <p class="text-red-600 select-none">"Username cannot be empty"</p> }
                    .into_any()
            }
            LoginInputStatus::PasswordEmpty => {
                view! { <p class="text-red-600 select-none">"Password cannot be empty"</p> }
                    .into_any()
            }
            LoginInputStatus::RecoveryKeyEmpty => {
                view! { <p class="text-red-600 select-none">"Recovery key cannot be empty"</p> }
                    .into_any()
            }
            LoginInputStatus::RecoveryKeyInvalid => view! {
                <p class="text-red-600 select-none">
                    "Recovery should be twelve groups of 4 characters seperated by spaces"
                </p>
            }
            .into_any(),
            LoginInputStatus::Error(err) => {
                view! { <p class="text-red-600 select-none">{err.clone()}</p> }.into_any()
            }
            LoginInputStatus::Loading => {
                view! { <p class="text-gray-600 select-none">"Logging in..."</p> }.into_any()
            }
        }
    }
}

impl From<LoginError> for LoginInputStatus {
    fn from(value: LoginError) -> Self {
        match value {
            LoginError::BackendError => {
                LoginInputStatus::Error("An unknown error occurred".to_string())
            }
            LoginError::InvalidCredentials => {
                LoginInputStatus::Error("Invalid username, password, or recovery key".to_string())
            }
            LoginError::NetworkError => LoginInputStatus::Error(
                "A network error occurred. Please check your connection and try again.".to_string(),
            ),
        }
    }
}

pub fn get_stuff_after_login(state: AppState, settings: Settings) {
    spawn_local(async move {
        match call_tauri_no_args("get_breadcrumbs").await {
            Ok(js_val) => {
                let breadcrumbs: Breadcrumbs = serde_wasm_bindgen::from_value(js_val).unwrap();

                if let Some(room_id) = breadcrumbs.recent_rooms.first() {
                    state.active_section.set(state.section_for_room(room_id));
                }

                state.breadcrums.set(breadcrumbs.clone());

                state.set_active_room_with_id(
                    breadcrumbs.recent_rooms.first().cloned(),
                    MainView::Chat,
                );
            }
            Err(err) => {
                log::error!("Error fetching breadcrumbs: {:?}", err);
            }
        }

        match get_server_order().await {
            Ok(order) => {
                state.server_order.set(order);
                state.apply_server_order();
            }
            Err(err) => {
                log::error!("Error fetching server order: {:?}", err);
            }
        };

        if let Err(e) = settings.get_all().await {
            log::error!("Failed to get settings from backend: {:?}", e);
        };

        state.data_initialized.set(true);
    });
}

#[component]
pub fn LoginPage(window: RwSignal<CurrentWindow>) -> impl IntoView {
    let state: AppState = expect_context();
    let settings: Settings = expect_context();

    let username = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let recovery_key = RwSignal::new(String::new());
    let status = RwSignal::new(LoginInputStatus::UsernameEmpty);

    let username_ref = NodeRef::<leptos::html::Input>::new();
    Effect::new(move || {
        if let Some(el) = username_ref.get() {
            let _ = el.focus();
        }
    });

    let check_key_format = move || {
        let key = recovery_key.get();

        let parts: Vec<&str> = key.split_whitespace().collect();
        if parts.len() != 12 {
            return false;
        }
        for part in parts {
            if part.len() != 4 {
                return false;
            }
        }
        true
    };

    let is_valid = move || {
        if username.get().is_empty() {
            status.set(LoginInputStatus::UsernameEmpty);
            return false;
        }
        if password.get().is_empty() {
            status.set(LoginInputStatus::PasswordEmpty);
            return false;
        }
        if recovery_key.get().is_empty() {
            status.set(LoginInputStatus::RecoveryKeyEmpty);
            return false;
        }
        if !check_key_format() {
            status.set(LoginInputStatus::RecoveryKeyInvalid);
            return false;
        }

        status.set(LoginInputStatus::Valid);
        true
    };

    let login = move |ev: SubmitEvent| {
        status.set(LoginInputStatus::Loading);

        ev.prevent_default();
        spawn_local(async move {
            let username = username.get_untracked();
            let password = password.get_untracked();
            let recovery_key = recovery_key.get_untracked();

            if username.is_empty() || password.is_empty() || recovery_key.is_empty() {
                return;
            }

            let args = serde_wasm_bindgen::to_value(&json!({
                "username": username,
                "password": password,
                "recovery_key": recovery_key
            }))
            .unwrap();

            match call_tauri("login", args).await {
                Ok(js_val) => {
                    let user_id: String = serde_wasm_bindgen::from_value(js_val).unwrap();

                    state.user_id.set(user_id);
                    window.set(CurrentWindow::Home);
                    get_stuff_after_login(state, settings);
                }
                Err(js_val) => {
                    let err: LoginError =
                        serde_wasm_bindgen::from_value(js_val).unwrap_or(LoginError::BackendError);

                    status.set(err.into());
                }
            };
        });
    };

    view! {
        <div class="relative flex items-center justify-center w-full text-(--accent-color) text-3xl font-bold pb-5">
            <button
                class="absolute left-0 -translate-y-1/2 hover:underline text-sm text-dim cursor-pointer select-none"
                on:click=move |_| window.set(CurrentWindow::HomeserverDiscovery)
            >
                "⟵ back"
            </button>
            "Login"
        </div>

        <form class="flex flex-col w-full" on:submit=login>
            <div class="flex flex-col">
                <ArgSpan text="Username" />
                <input
                    id="username-input"
                    placeholder="luke"
                    class="p-2.5 text-xl rounded-lg select-none w-full bg-(--ui-floating-bg) placeholder:text-muted text-normal border border-(--tile-border-color) outline-none focus:border-(--accent-color) focus:bg-(--ui-floating-hover-bg) mb-5"
                    node_ref=username_ref
                    on:input=move |ev| username.set(event_target_value(&ev))
                />
            </div>

            <div class="flex flex-col">
                <ArgSpan text="Password" />
                <input
                    id="password-input"
                    placeholder="••••••••"
                    class="p-2.5 text-xl rounded-lg select-none w-full bg-(--ui-floating-bg) placeholder:text-muted text-normal border border-(--tile-border-color) outline-none focus:border-(--accent-color) focus:bg-(--ui-floating-hover-bg) mb-5"
                    on:input=move |ev| password.set(event_target_value(&ev))
                    type="password"
                />
            </div>

            <div class="flex flex-col">
                <ArgSpan text="Recovery Key" />
                <input
                    id="recovery-key-input"
                    placeholder="Es9X xxxx xxxx..."
                    class="p-2.5 text-xl rounded-lg select-none w-full bg-(--ui-floating-bg) placeholder:text-muted text-normal border border-(--tile-border-color) outline-none focus:border-(--accent-color) focus:bg-(--ui-floating-hover-bg)"
                    on:input=move |ev| recovery_key.set(event_target_value(&ev))
                    type="password"
                />
            </div>

            <div class="p-2">{move || status.get().render()}</div>

            <button
                type="submit"
                class="px-5 py-2.5 rounded-md border-none select-none transition-colors"
                class=("bg-(--confirm-color)", move || is_valid())
                class=("text-(--confirm-text-color)", move || is_valid())
                class=("cursor-pointer", move || is_valid())
                class=("bg-(--text-muted)", move || !is_valid())
                class=("text-normal", move || !is_valid())
                class=("cursor-not-allowed", move || !is_valid())
                class=("hover:bg-(--confirm-hover-color)", move || is_valid())
                disabled=move || !is_valid()
            >
                "Log in"
            </button>
        </form>
    }
}

#[derive(Clone)]
enum DiscoveryStatus {
    Idle,
    Checking,
    Found,
    NotFound,
}

impl DiscoveryStatus {
    fn render(self) -> impl IntoView {
        match self {
            DiscoveryStatus::Idle => {
                view! { <p class="text-gray-600 select-none">"Enter a homeserver URL"</p> }
            }
            DiscoveryStatus::Checking => {
                view! { <p class="text-gray-600 select-none">"Checking server..."</p> }
            }
            DiscoveryStatus::Found => {
                view! { <p class="text-green-600 select-none">"Server found"</p> }
            }
            DiscoveryStatus::NotFound => {
                view! { <p class="text-red-600 select-none">"Server not found"</p> }
            }
        }
    }
}

#[component]
pub fn HomeserverDiscoveryPage(window: RwSignal<CurrentWindow>) -> impl IntoView {
    let initial_text = "erik-is.gay".to_string();

    let text = RwSignal::new(initial_text);
    let is_valid = RwSignal::new(false);
    let status_message = RwSignal::new(DiscoveryStatus::Idle);

    let try_home_server = move || {
        let current_value = text.get();

        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&json!({ "url": current_value })).unwrap();

            match call_tauri("choose_home_server", args).await {
                Ok(url) => {
                    if url == *text.read_untracked() {
                        is_valid.set(true);
                        status_message.set(DiscoveryStatus::Found);
                    } else {
                        is_valid.set(false);
                        status_message.set(DiscoveryStatus::NotFound);
                    }
                }
                Err(_) => {
                    is_valid.set(false);
                    status_message.set(DiscoveryStatus::NotFound);
                }
            }
        });
    };

    Effect::new(move || {
        let current_value = text.get();

        if current_value.is_empty() {
            is_valid.set(false);
            status_message.set(DiscoveryStatus::Idle);
        } else {
            try_home_server();
        }
    });

    let input_ref: NodeRef<Input> = NodeRef::new();

    Effect::new(move || {
        if let Some(el) = input_ref.get() {
            move_caret_to_end(&el);
        }
    });

    let choose_home_server = async move || {
        let chosen_server = text.get_untracked();

        let args = serde_wasm_bindgen::to_value(&json!({ "url": chosen_server })).unwrap();

        // TODO: refactor code be less duplicate, see discovery.rs for reference
        call_tauri("choose_home_server", args).await.unwrap();
    };

    view! {
        <span class="text-(--accent-color) text-3xl font-bold pb-5 text-center select-none">
            "Discovery"
        </span>

        // TODO: Add a link to what a home server is, and maybe some popular ones to choose from
        <ArgSpan text="Homeserver" />
        <input
            type="text"
            node_ref=input_ref
            placeholder="example.org"
            class="p-2.5 text-xl rounded-lg select-none w-full bg-(--ui-floating-bg) placeholder:text-muted text-normal border border-(--tile-border-color) outline-none focus:border-(--accent-color) focus:bg-(--ui-floating-hover-bg)"
            autofocus
            on:input=move |ev| {
                status_message.set(DiscoveryStatus::Checking);
                text.set(event_target_value(&ev));
                is_valid.set(false);
                try_home_server();
            }
            on:mount=move |el: HtmlInputElement| {
                let _ = el.focus();
                let len = el.value().encode_utf16().count() as u32;
                let _ = el.set_selection_range(len, len);
            }
            prop:value=text
        />

        <div class="p-2">{move || status_message.get().render()}</div>

        <button
            class="px-5 py-2.5 rounded-md border-none select-none transition-colors w-full"
            class=("bg-(--confirm-color)", move || is_valid.get())
            class=("text-(--confirm-text-color)", move || is_valid.get())
            class=("cursor-pointer", move || is_valid.get())
            class=("bg-(--text-muted)", move || !is_valid.get())
            class=("text-normal", move || !is_valid.get())
            class=("cursor-not-allowed", move || !is_valid.get())
            class=("hover:bg-(--confirm-hover-color)", move || is_valid.get())
            disabled=move || !is_valid.get()
            on:click=move |_| {
                spawn_local(async move {
                    choose_home_server().await;
                    window.set(CurrentWindow::Login);
                })
            }
        >
            "Continue to login"
        </button>
    }
}

#[component]
pub fn Authentication() -> impl IntoView {
    let state: AppState = expect_context();
    let window = state.current_window;

    view! {
        <SystemButtonsInTile />
        <SingleFloatingTile class="p-5">
            <div class="min-w-100 flex flex-col">
                {move || match window.get() {
                    CurrentWindow::HomeserverDiscovery => {
                        view! { <HomeserverDiscoveryPage window=window /> }.into_any()
                    }
                    CurrentWindow::Login => view! { <LoginPage window=window /> }.into_any(),
                    _ => view! { <LoginPage window=window /> }.into_any(),
                }}
            </div>
        </SingleFloatingTile>
    }
}
