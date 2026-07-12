mod app;
mod components;
mod hooks;
mod logger;
mod redact_mode;
mod state;
mod tauri_functions;

pub use crate::logger::{debug, error, info, trace, warn};

use app::*;
use components::log_view::LogView;
use leptos::prelude::*;

/// True when the window was opened with `?view=logs` (the log viewer window).
fn is_log_view() -> bool {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .map(|search| search.contains("view=logs"))
        .unwrap_or(false)
}

fn main() {
    console_error_panic_hook::set_once();

    if is_log_view() {
        // Deliberately skip installing `FrontendLogger` here: the log window is
        // itself a frontend, and forwarding its own logs to the backend would
        // loop them straight back into this same view.
        mount_to_body(|| {
            view! { <LogView /> }
        });
        return;
    }

    logger::FrontendLogger::init(log::LevelFilter::Trace).expect("Failed to initialize logger");
    mount_to_body(|| {
        view! { <App /> }
    })
}
