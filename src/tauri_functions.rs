use leptos::task::spawn_local;
use serde::Serialize;
use serde_json::json;

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

#[derive(Clone, Debug, PartialEq)]
pub struct MemberShip {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl MemberShip {
    /// Creates a `room` instance for matching
    pub fn room(room_id: String) -> Self {
        Self {
            user_id: room_id,
            display_name: Some("room".into()),
            avatar_url: None,
        }
    }

    pub fn get_name(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| self.user_id.clone())
    }
}

pub async fn get_members(room_id: String) -> Result<Vec<MemberShip>, String> {
    let args = serde_wasm_bindgen::to_value(&json!(
            {
        "room_id": room_id
    }
        ))
    .map_err(|_| "Failed to construct args".to_string())?;

    let js_val = call_tauri("get_members_for_room", args)
        .await
        .map_err(|e| format!("Failed to get members: {:?}", e))?;

    let members: Vec<(String, Option<String>, Option<String>)> =
        serde_wasm_bindgen::from_value(js_val)
            .map_err(|e| format!("Failed to deserialize answer: {:?}", e))?;

    Ok(members
        .iter()
        .map(|(u, d, a)| MemberShip {
            user_id: u.into(),
            display_name: d.clone().map(|v| v.into()),
            avatar_url: a.clone().map(|v| v.into()),
        })
        .collect())
}
