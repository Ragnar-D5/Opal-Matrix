mod app;
mod components;
mod hooks;
mod logger;
mod state;
mod tauri_functions;

pub use crate::logger::{debug, error, info, trace, warn};

use app::*;
use leptos::prelude::*;

fn main() {
    logger::FrontendLogger::init(log::LevelFilter::Trace).expect("Failed to initialize logger");
    console_error_panic_hook::set_once();
    mount_to_body(|| {
        view! { <App /> }
    })
}
