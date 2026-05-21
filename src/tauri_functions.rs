use leptos::{prelude::Get, task::spawn_local};
use serde::Serialize;
use serde_json::json;
use shared::{
    api::{FetchMessagesResponse, LinkPreviewResponse},
    commands::Command,
    timeline::RichTextSpan,
    timeline::UiTimelineItem,
};

use crate::{
    app::{call_tauri, call_tauri_no_args},
    state::AppState,
};

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
    pub fn room(room_id: String) -> Self {
        Self {
            user_id: room_id,
            display_name: Some("room".into()),
            avatar_url: None,
        }
    }

    fn is_room(&self, state: AppState) -> bool {
        let Some(rid) = state.active_room_id.get() else {
            return false;
        };
        rid == self.user_id
    }

    pub fn get_name(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| self.user_id.clone())
    }

    pub fn to_span(&self, state: AppState) -> RichTextSpan {
        if self.is_room(state) {
            return RichTextSpan::RoomMention;
        }

        RichTextSpan::UserMention {
            user_id: self.user_id.clone(),
            display_name: self.get_name(),
        }
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
            display_name: d.clone(),
            avatar_url: a.clone(),
        })
        .collect())
}

pub async fn commit_message(message: String, room_id: String) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "html": message, "room_id": room_id }))
        .map_err(|e| format!("Failed to construct args: {e}"))?;

    call_tauri("commit_message", args)
        .await
        .map_err(|e| format!("Failed to commit message: {:?}", e))?;

    Ok(())
}

pub async fn set_backend_room_id(room_id: Option<String>) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "room_id": room_id }))
        .map_err(|e| format!("Failed to construct args: {e}"))?;

    call_tauri("set_room_id", args)
        .await
        .map_err(|e| format!("Failed to set active room: {:?}", e))?;

    Ok(())
}

pub async fn set_focused_in_backend(focused: bool) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "focused": focused }))
        .map_err(|e| format!("Failed to construct args: {e}"))?;

    call_tauri("set_frontend_focused", args)
        .await
        .map_err(|e| format!("Failed to set focused: {:?}", e))?;

    Ok(())
}

pub async fn fetch_preview_data(url: String) -> Result<LinkPreviewResponse, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "url": url })).map_err(|e| e.to_string())?;

    let js_val = call_tauri("get_url_preview", args)
        .await
        .map_err(|e| format!("Failed to fetch preview data: {:?}", e))?;

    let preview: LinkPreviewResponse = serde_wasm_bindgen::from_value(js_val)
        .map_err(|e| format!("Failed to deserialize preview data: {:?}", e))?;

    Ok(preview)
}

pub async fn get_commands() -> Result<Vec<Command>, String> {
    match call_tauri_no_args("get_commands").await {
        Ok(result) => serde_wasm_bindgen::from_value(result)
            .map_err(|e| format!("Failed to deserialize commands: {:?}", e)),
        Err(e) => Err(format!("Failed to fetch commands: {:?}", e)),
    }
}

pub async fn get_timeline(room_id: &str) -> Result<Vec<UiTimelineItem>, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "room_id": room_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let res = call_tauri("get_timeline", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let json_string: String = js_sys::JSON::stringify(&res)
        .map_err(|e| format!("Failed to convert response to string: {:?}", e))?
        .into();

    let res: Vec<UiTimelineItem> = serde_json::from_str(&json_string)
        .map_err(|e| format!("Failed to parse JSON response: {:?}", e))?;

    Ok(res)
}

pub async fn scroll_up(room_id: &str) -> Result<bool, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "room_id": room_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let res = call_tauri("scroll_up", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let has_more: bool = serde_wasm_bindgen::from_value(res)
        .map_err(|e| format!("Failed to parse response: {:?}", e))?;

    Ok(has_more)
}
