use serde::Serialize;
use std::fs::File;
use std::io::Write;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// Append-only JSON-lines log of frontend<->backend IPC traffic (event/call names and payload
/// sizes in bytes). Only managed as app state on desktop debug builds, see `setup_builder`.
pub struct IpcTrafficLog(Mutex<File>);

impl IpcTrafficLog {
    /// `start_time` should match the timestamp used for the main log file name, so the two
    /// files can be correlated to the same app run.
    #[cfg(all(desktop, debug_assertions))]
    pub fn init(log_dir: &std::path::Path, start_time: &str) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join(format!("ipc-traffic-{start_time}.jsonl")))?;
        Ok(Self(Mutex::new(file)))
    }

    fn write_line(&self, line: &[u8]) {
        let Ok(mut file) = self.0.lock() else {
            return;
        };
        let _ = file.write_all(line);
        let _ = file.write_all(b"\n");
    }
}

#[derive(Serialize)]
#[serde(tag = "direction", rename_all = "snake_case")]
enum Entry<'a> {
    Event {
        name: &'a str,
        bytes: usize,
    },
    Call {
        name: &'a str,
        request_bytes: usize,
        response_bytes: usize,
        ok: bool,
    },
}

#[derive(Serialize)]
struct Line<'a> {
    timestamp: String,
    #[serde(flatten)]
    entry: Entry<'a>,
}

fn write_entry(handle: &AppHandle, entry: Entry) {
    let Some(log) = handle.try_state::<IpcTrafficLog>() else {
        return;
    };
    let line = Line {
        timestamp: chrono::Local::now()
            .format("%Y-%m-%d %H:%M:%S.%3f")
            .to_string(),
        entry,
    };
    if let Ok(json) = serde_json::to_vec(&line) {
        log.write_line(&json);
    }
}

/// Backend -> frontend event emission.
pub fn log_event(handle: &AppHandle, name: &str, bytes: usize) {
    write_entry(handle, Entry::Event { name, bytes });
}

/// Frontend -> backend command call.
pub fn log_call(
    handle: &AppHandle,
    name: &str,
    request_bytes: usize,
    response_bytes: usize,
    ok: bool,
) {
    write_entry(
        handle,
        Entry::Call {
            name,
            request_bytes,
            response_bytes,
            ok,
        },
    );
}

/// Called by the frontend once per `invoke()` to report the call it just made.
#[tauri::command(rename_all = "snake_case")]
pub fn log_ipc_call(
    app_handle: AppHandle,
    cmd: String,
    request_bytes: usize,
    response_bytes: usize,
    ok: bool,
) {
    log_call(&app_handle, &cmd, request_bytes, response_bytes, ok);
}
