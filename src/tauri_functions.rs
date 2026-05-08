use leptos::task::spawn_local;
use serde::Serialize;

use crate::app::call_tauri;

#[derive(Serialize)]
struct ReadMarkerArgs {
    room_id: String,
    event_id: String,
}

pub fn send_marker(room_id: String, event_id: String) {
    spawn_local(async move {
        let args = match serde_wasm_bindgen::to_value(&ReadMarkerArgs { room_id, event_id }) {
            Ok(a) => a,
            Err(_) => return,
        };
        let _ = call_tauri("send_read_marker", args).await;
    });
}
