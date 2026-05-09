use chrono::Local;
pub use log::{debug, error, info, trace, warn};

use log::{LevelFilter, Metadata, Record};
use serde::Serialize;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;
use web_sys::console;

#[derive(Serialize, Debug, Clone)]
pub struct BackendLogPayload {
    pub level: String,
    pub timestamp: String,
    pub path: String,
    pub line: Option<u32>,
    pub message: String,
}

pub struct FrontendLogger;

impl FrontendLogger {
    pub fn init(level: LevelFilter) -> Result<(), log::SetLoggerError> {
        log::set_logger(&LOGGER)?;
        log::set_max_level(level);
        Ok(())
    }
}

static LOGGER: FrontendLogger = FrontendLogger;

impl log::Log for FrontendLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let path = record.file().unwrap_or("unknown").to_string();
        let line = record.line();
        let message = record.args().to_string();
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S.%3f").to_string();
        let level = record.level().to_string();

        let formatted = format!(
            "[{level}] {path}:{} {message} {timestamp}",
            line.unwrap_or(0)
        );
        console::log_1(&JsValue::from_str(&formatted));

        let payload = BackendLogPayload {
            level,
            timestamp,
            path,
            line,
            message,
        };

        spawn_local(async move {
            let args = match serde_wasm_bindgen::to_value(&payload) {
                Ok(val) => val,
                Err(err) => {
                    console::warn_1(&JsValue::from_str(&format!(
                        "Failed to serialize backend log payload: {err:?}"
                    )));
                    return;
                }
            };

            if let Err(err) = crate::app::call_tauri("backend_log", args).await {
                console::warn_1(&JsValue::from_str(&format!(
                    "backend_log call failed: {err:?}"
                )));
            }
        });
    }

    fn flush(&self) {}
}
