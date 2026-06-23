use klipy::{MediaItem, Page};
use leptos::task::spawn_local;
use serde_json::json;
use shared::{
    account_data::ServerOrder,
    api::{AudioDevice, FileMetadata, GetTimelineResult, LinkPreviewResponse, ScrollDirection},
    commands::Command,
    profile::UserProfile,
    timeline::UiMediaSource,
};

use crate::app::{call_tauri, call_tauri_no_args};

pub async fn commit_message(
    message: String,
    room_id: &str,
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

pub async fn edit_message(message: String, room_id: &str, event_id: String) -> Result<(), String> {
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

pub async fn get_timeline(
    room_id: &str,
    event_id: Option<String>,
) -> Result<GetTimelineResult, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "room_id": room_id, "event_id": event_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let res = call_tauri("get_timeline", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let json_string: String = js_sys::JSON::stringify(&res)
        .map_err(|e| format!("Failed to convert response to string: {:?}", e))?
        .into();

    let res: GetTimelineResult = serde_json::from_str(&json_string)
        .map_err(|e| format!("Failed to parse JSON response: {:?}", e))?;

    Ok(res)
}

pub async fn scroll_timeline(
    timeline_id: &str,
    scroll_direction: ScrollDirection,
) -> Result<bool, String> {
    let args = serde_wasm_bindgen::to_value(
        &json!({ "timeline_id": timeline_id, "direction": scroll_direction }),
    )
    .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let res = call_tauri("scroll_timeline", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let has_more: bool = serde_wasm_bindgen::from_value(res)
        .map_err(|e| format!("Failed to parse response: {:?}", e))?;

    Ok(has_more)
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

pub async fn send_attachment(file: FileMetadata, room_id: &str) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "file": file, "room_id": room_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    call_tauri("send_attachment", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    Ok(())
}

pub async fn save_file_to_picked_dest(
    source: UiMediaSource,
    file_name: &str,
) -> Result<Option<String>, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "source": source, "file_name": file_name }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let res = call_tauri("save_file_to_picked_dest", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let path: Option<String> = serde_wasm_bindgen::from_value(res)
        .map_err(|e| format!("Failed to parse response: {:?}", e))?;

    Ok(path)
}

pub async fn get_user_profile(user_id: &str) -> Result<UserProfile, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "user_id": user_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let res = call_tauri("get_user_profile", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let profile: UserProfile = serde_wasm_bindgen::from_value(res)
        .map_err(|e| format!("Failed to parse response: {:?}", e))?;

    Ok(profile)
}

pub async fn get_server_order() -> Result<ServerOrder, String> {
    let res = call_tauri_no_args("get_server_order")
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let server_order: ServerOrder = serde_wasm_bindgen::from_value(res)
        .map_err(|e| format!("Failed to parse response: {:?}", e))?;

    Ok(server_order)
}

pub async fn delete_message(room_id: &str, event_id: &str) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "room_id": room_id, "event_id": event_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    call_tauri("delete_message", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    Ok(())
}

pub async fn indicate_typing(room_id: &str, is_typing: bool) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "room_id": room_id, "is_typing": is_typing }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    call_tauri("indicate_typing", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    Ok(())
}

pub async fn get_gifs(search_term: String, page: u32) -> Result<Page<MediaItem>, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "search_term": search_term, "page": page }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let res = call_tauri("search_gifs", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let json_string: String = js_sys::JSON::stringify(&res)
        .map_err(|e| format!("Failed to convert response to string: {:?}", e))?
        .into();

    serde_json::from_str(&json_string)
        .map_err(|e| format!("Failed to parse JSON response: {:?}", e))
}

pub fn save_name_color(color: &str) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "color": color }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    spawn_local(async move {
        if let Err(e) = call_tauri("save_namecolor", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });

    Ok(())
}

pub fn close_window() {
    spawn_local(async move {
        if let Err(e) = call_tauri_no_args("close_window").await {
            log::error!("Failed to close window: {:?}", e);
        }
    });
}

pub fn minimize_window() {
    spawn_local(async move {
        if let Err(e) = call_tauri_no_args("minimize_window").await {
            log::error!("Failed to minimize window: {:?}", e);
        }
    });
}

pub fn toggle_fullscreen() {
    spawn_local(async move {
        if let Err(e) = call_tauri_no_args("toggle_fullscreen").await {
            log::error!("Failed to toggle fullscreen: {:?}", e);
        }
    });
}

pub fn save_banner_color(color: &str) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "color": color }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    spawn_local(async move {
        if let Err(e) = call_tauri("save_bannercolor", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });

    Ok(())
}

pub fn save_displayname(display_name: &str, room_id: Option<String>) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "name": display_name, "room_id": room_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    spawn_local(async move {
        if let Err(e) = call_tauri("save_displayname", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });

    Ok(())
}

// pub fn save_avatar_image(image_data: Vec<u8>) -> Result<(), String> {
//     let args = serde_wasm_bindgen::to_value(&json!({ "image_data": image_data }))
//         .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

//     spawn_local(async move {
//         if let Err(e) = call_tauri("save_avatar", args).await {
//             log::error!("Tauri call failed: {:?}", e);
//         }
//     });

//     Ok(())
// }

pub fn get_audio_devices() {
    spawn_local(async move {
        if let Err(e) = call_tauri_no_args("get_audio_devices").await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });
}

pub fn set_output_device(id: String) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "id": id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    spawn_local(async move {
        if let Err(e) = call_tauri("set_output_device", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });

    Ok(())
}

pub fn set_input_device(id: String) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "id": id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    spawn_local(async move {
        if let Err(e) = call_tauri("set_input_device", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });

    Ok(())
}
