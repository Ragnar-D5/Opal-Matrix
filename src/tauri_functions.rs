use serde_json::json;
use shared::{
    api::{FileMetadata, LinkPreviewResponse},
    commands::Command,
    timeline::UiTimelineItem,
    user_profile::UserProfile,
};

use crate::app::{call_tauri, call_tauri_no_args};

pub async fn commit_message(
    message: String,
    room_id: String,
    replies_to: Option<String>,
) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(
        &json!({ "html": message, "room_id": room_id, "replies_to": replies_to }),
    )
    .map_err(|e| format!("Failed to construct args: {e}"))?;

    call_tauri("commit_message", args)
        .await
        .map_err(|e| format!("Failed to commit message: {:?}", e))?;

    Ok(())
}

pub async fn edit_message(
    message: String,
    room_id: String,
    event_id: String,
) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(
        &json!({ "html": message, "room_id": room_id, "event_id": event_id }),
    )
    .map_err(|e| format!("Failed to construct args: {e}"))?;

    call_tauri("edit_message", args)
        .await
        .map_err(|e| format!("Failed to edit message: {:?}", e))?;

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

pub async fn get_members_for_room(room_id: &str) -> Result<Vec<UserProfile>, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "room_id": room_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let res = call_tauri("get_members_for_room", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let members: Vec<UserProfile> = serde_wasm_bindgen::from_value(res)
        .map_err(|e| format!("Failed to parse response: {:?}", e))?;

    Ok(members)
}

pub async fn toggle_reaction(room_id: &str, event_id: &str, reaction: &str) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(
        &json!({ "room_id": room_id, "event_id": event_id, "reaction": reaction }),
    )
    .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    call_tauri("toggle_reaction", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    Ok(())
}

pub async fn pick_files() -> Result<Vec<FileMetadata>, String> {
    let res = call_tauri_no_args("open_file_dialog")
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let paths: Vec<FileMetadata> = serde_wasm_bindgen::from_value(res)
        .map_err(|e| format!("Failed to parse response: {:?}", e))?;

    Ok(paths)
}
