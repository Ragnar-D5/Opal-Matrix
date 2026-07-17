use klipy::{MediaItem, Page};
use leptos::prelude::*;
use leptos::task::spawn_local;
use ruma::{
    EventId, OwnedEventId, OwnedRoomId, RoomId, UserId, directory::RoomTypeFilter,
    events::room::MediaSource,
};
use serde_json::json;
use shared::{
    UiThumbnailSettings,
    account_data::ServerOrder,
    api::{
        FileMetadata, GetTimelineResult, LinkPreviewResponse, ScrollDirection, SearchParameters,
    },
    commands::Command,
    profile::UserProfile,
    sidebar::RoomExtraInfo,
    timeline::{UiMediaSource, UiTimelineItem},
};
use uuid::Uuid;
use wasm_bindgen::{JsCast, JsValue};

use crate::hooks::{Channel, call_tauri, call_tauri_no_args, call_tauri_with_channel};

pub async fn send_message(
    message: String,
    room_id: &RoomId,
    replies_to: Option<OwnedEventId>,
) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(
        &json!({ "html": message, "room_id": room_id, "replies_to": replies_to }),
    )
    .map_err(|e| format!("Failed to construct args: {e}"))?;

    call_tauri("send_message", args)
        .await
        .map_err(|e| format!("Failed to commit message: {:?}", e))?;

    Ok(())
}

pub async fn edit_message(
    message: String,
    room_id: &RoomId,
    event_id: &EventId,
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

pub async fn set_backend_room_id(room_id: Option<OwnedRoomId>) -> Result<(), String> {
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
    room_id: &RoomId,
    event_id: Option<OwnedEventId>,
    channel: &Channel,
) -> Result<GetTimelineResult, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "room_id": room_id, "event_id": event_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let res = call_tauri_with_channel("get_timeline", args, channel)
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

pub fn toggle_reaction(room_id: &RoomId, event_id: &EventId, reaction: &str) {
    let args = match serde_wasm_bindgen::to_value(
        &json!({ "room_id": room_id, "event_id": event_id, "reaction": reaction }),
    ) {
        Ok(value) => value,
        Err(e) => {
            log::error!("Failed to serialize request: {:?}", e);
            return;
        }
    };

    spawn_local(async move {
        if let Err(e) = call_tauri("toggle_reaction", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });
}

pub async fn pick_files() -> Result<Vec<FileMetadata>, String> {
    let res = call_tauri_no_args("open_file_dialog")
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let paths: Vec<FileMetadata> = serde_wasm_bindgen::from_value(res)
        .map_err(|e| format!("Failed to parse response: {:?}", e))?;

    Ok(paths)
}

pub async fn send_attachment(file: FileMetadata, room_id: &RoomId) -> Result<(), String> {
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

pub async fn get_user_profile(user_id: &UserId) -> Result<UserProfile, String> {
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

pub async fn delete_message(room_id: &RoomId, event_id: &EventId) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "room_id": room_id, "event_id": event_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    call_tauri("delete_message", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    Ok(())
}

pub async fn indicate_typing(room_id: &RoomId, is_typing: bool) -> Result<(), String> {
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

pub fn close_window(minimize_to_tray: bool) {
    let args = match serde_wasm_bindgen::to_value(&json!({ "minimize_to_tray": minimize_to_tray }))
    {
        Ok(args) => args,
        Err(e) => {
            log::error!("Failed to serialize request: {:?}", e);
            return;
        }
    };

    spawn_local(async move {
        if let Err(e) = call_tauri("close_window", args).await {
            log::error!("Failed to close window: {:?}", e);
        }
    });
}

/// Opens (or focuses) the separate live log window.
pub fn open_log_window() {
    spawn_local(async move {
        if let Err(e) = call_tauri_no_args("open_log_window").await {
            log::error!("Failed to open log window: {:?}", e);
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

pub fn save_displayname(display_name: &str, room_id: Option<OwnedRoomId>) -> Result<(), String> {
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

pub fn search_rooms(parameters: SearchParameters, search_id: Uuid) {
    let args = match serde_wasm_bindgen::to_value(
        &json!({ "parameters": parameters, "search_id": search_id }),
    ) {
        Ok(value) => value,
        Err(e) => {
            log::error!("Failed to serialize request: {:?}", e);
            return;
        }
    };

    spawn_local(async move {
        if let Err(e) = call_tauri("search_rooms", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });
}

pub fn join_call(room_id: &RoomId) {
    let args = match serde_wasm_bindgen::to_value(&json!({ "room_id": room_id })) {
        Ok(value) => value,
        Err(e) => {
            log::error!("Failed to serialize request: {:?}", e);
            return;
        }
    };

    spawn_local(async move {
        if let Err(e) = call_tauri("join_matrixrtc_call", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });
}

pub fn leave_call(room_id: &RoomId) {
    let args = match serde_wasm_bindgen::to_value(&json!({ "room_id": room_id })) {
        Ok(value) => value,
        Err(e) => {
            log::error!("Failed to serialize request: {:?}", e);
            return;
        }
    };

    spawn_local(async move {
        if let Err(e) = call_tauri("leave_matrixrtc_call", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });
}

pub async fn get_pinned_events(room_id: &RoomId) -> Result<Vec<UiTimelineItem>, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "room_id": room_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let res = call_tauri("get_pinned_events", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let json_string: String = js_sys::JSON::stringify(&res)
        .map_err(|e| format!("Failed to convert response to string: {:?}", e))?
        .into();

    serde_json::from_str(&json_string)
        .map_err(|e| format!("Failed to parse JSON response: {:?}", e))
}

pub fn pin_event(room_id: &RoomId, event_id: &EventId) {
    let args =
        match serde_wasm_bindgen::to_value(&json!({ "room_id": room_id, "event_id": event_id })) {
            Ok(value) => value,
            Err(e) => {
                log::error!("Failed to serialize request: {:?}", e);
                return;
            }
        };

    spawn_local(async move {
        if let Err(e) = call_tauri("pin_event", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });
}

pub fn unpin_event(room_id: &RoomId, event_id: &EventId) {
    let args =
        match serde_wasm_bindgen::to_value(&json!({ "room_id": room_id, "event_id": event_id })) {
            Ok(value) => value,
            Err(e) => {
                log::error!("Failed to serialize request: {:?}", e);
                return;
            }
        };

    spawn_local(async move {
        if let Err(e) = call_tauri("unpin_event", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });
}

pub fn get_update_status() {
    spawn_local(async move {
        if let Err(e) = call_tauri_no_args("get_update_status").await {
            log::error!("Failed to get update status: {:?}", e)
        }
    });
}

pub fn check_for_update() {
    spawn_local(async move {
        if let Err(e) = call_tauri_no_args("check_for_update").await {
            log::error!("Failed to check for update: {:?}", e)
        }
    });
}

pub async fn get_app_version() -> Result<String, String> {
    let res = call_tauri_no_args("get_app_version")
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let json_string: String = js_sys::JSON::stringify(&res)
        .map_err(|e| format!("Failed to convert response to string: {:?}", e))?
        .into();

    let version: String = serde_json::from_str(&json_string)
        .map_err(|e| format!("Failed to parse JSON response: {:?}", e))?;

    Ok(version)
}

pub fn download_update() {
    spawn_local(async move {
        if let Err(e) = call_tauri_no_args("download_update").await {
            log::error!("Failed to download update: {:?}", e)
        }
    });
}

pub fn recheck_update() {
    spawn_local(async move {
        if let Err(e) = call_tauri_no_args("recheck_update").await {
            log::error!("Failed to recheck update: {:?}", e)
        }
    });
}

pub fn install_update() {
    spawn_local(async move {
        if let Err(e) = call_tauri_no_args("install_update").await {
            log::error!("Failed to install update: {:?}", e)
        }
    });
}

pub fn change_screen_scaling(scale_factor: f64) {
    let args = match serde_wasm_bindgen::to_value(&json!({ "scale_factor": scale_factor / 100.0 }))
    {
        Ok(value) => value,
        Err(e) => {
            log::error!("Failed to serialize request: {:?}", e);
            return;
        }
    };

    spawn_local(async move {
        if let Err(e) = call_tauri("change_screen_scaling", args).await {
            log::error!("Tauri call failed: {:?}", e);
        }
    });
}

pub async fn get_extra_room_info(room_id: &RoomId) -> Result<RoomExtraInfo, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "room_id": room_id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let result = call_tauri("get_extra_room_info", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let json_string: String = js_sys::JSON::stringify(&result)
        .map_err(|e| format!("Failed to stringify result: {:?}", e))?
        .into();

    serde_json::from_str(&json_string).map_err(|e| format!("Failed to parse result: {:?}", e))
}

pub async fn get_versions() -> Result<Vec<String>, String> {
    let result = call_tauri(
        "get_versions",
        serde_wasm_bindgen::to_value(&json!({}))
            .map_err(|e| format!("Failed to serialize request: {:?}", e))?,
    )
    .await
    .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let json_string: String = js_sys::JSON::stringify(&result)
        .map_err(|e| format!("Failed to stringify result: {:?}", e))?
        .into();

    serde_json::from_str(&json_string).map_err(|e| format!("Failed to parse result: {:?}", e))
}

pub async fn get_version(version: &str) -> Result<String, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "version": version }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let result = call_tauri("get_version", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let json_string: String = js_sys::JSON::stringify(&result)
        .map_err(|e| format!("Failed to stringify result: {:?}", e))?
        .into();

    serde_json::from_str(&json_string).map_err(|e| format!("Failed to parse result: {:?}", e))
}

pub async fn open_room_search(
    id: Uuid,
    room_types: Vec<RoomTypeFilter>,
    channel: &Channel,
) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "id": id, "room_types": room_types }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    call_tauri_with_channel("open_room_search", args, channel)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    Ok(())
}

/// Returns `true` once the search has reached the last page of results.
pub async fn search_room_directory(id: Uuid, term: Option<String>) -> Result<bool, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "id": id, "term": term }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let result = call_tauri("search_room_directory", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let json_string: String = js_sys::JSON::stringify(&result)
        .map_err(|e| format!("Failed to stringify result: {:?}", e))?
        .into();

    serde_json::from_str(&json_string).map_err(|e| format!("Failed to parse result: {:?}", e))
}

/// Returns `true` once the search has reached the last page of results.
pub async fn load_more_room_search(id: Uuid) -> Result<bool, String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "id": id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    let result = call_tauri("load_more_room_search_results", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let json_string: String = js_sys::JSON::stringify(&result)
        .map_err(|e| format!("Failed to stringify result: {:?}", e))?
        .into();

    serde_json::from_str(&json_string).map_err(|e| format!("Failed to parse result: {:?}", e))
}

pub async fn close_room_search(id: Uuid) -> Result<(), String> {
    let args = serde_wasm_bindgen::to_value(&json!({ "id": id }))
        .map_err(|e| format!("Failed to serialize request: {:?}", e))?;

    call_tauri("close_room_search", args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    Ok(())
}
/// Sniffs the MIME type from the leading magic bytes. Blob URLs serve exactly the
/// type set on the blob: raster images render in `<img>` even with a generic type,
/// but SVG (and `<video>` sources) only work when the type is correct.
fn detect_content_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        "image/jpeg"
    } else if bytes.starts_with(b"\x89PNG") {
        "image/png"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        match &bytes[8..12] {
            b"avif" | b"avis" => "image/avif",
            b"heic" | b"heix" | b"mif1" => "image/heic",
            _ => "video/mp4",
        }
    } else if bytes.starts_with(b"BM") {
        "image/bmp"
    } else if bytes.starts_with(b"\x00\x00\x01\x00") {
        "image/x-icon"
    } else if bytes.starts_with(b"\x1A\x45\xDF\xA3") {
        "video/webm"
    } else if bytes.starts_with(b"OggS") {
        "video/ogg"
    } else if String::from_utf8_lossy(&bytes[..bytes.len().min(512)])
        .trim_start_matches('\u{feff}')
        .trim_start()
        .starts_with("<?xml")
        || String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).contains("<svg")
    {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

fn bytes_to_blob_url(buffer: js_sys::ArrayBuffer) -> Result<String, String> {
    use js_sys::{Array, Uint8Array};
    use web_sys::{Blob, BlobPropertyBag, Url};

    let bytes = Uint8Array::new(&buffer);
    let head = bytes.slice(0, bytes.length().min(512)).to_vec();

    let opts = BlobPropertyBag::new();
    opts.set_type(detect_content_type(&head));

    let parts = Array::of1(&buffer);
    let blob = Blob::new_with_u8_array_sequence_and_options(&parts, &opts)
        .map_err(|e| format!("Failed to create blob: {:?}", e))?;

    Url::create_object_url_with_blob(&blob)
        .map_err(|e| format!("Failed to create object URL: {:?}", e))
}

/// Invokes a media-fetching command and converts its raw `ArrayBuffer` response
/// into a blob object URL.
async fn fetch_blob_url(cmd: &str, args: JsValue) -> Result<String, String> {
    let res = call_tauri(cmd, args)
        .await
        .map_err(|e| format!("Tauri call failed: {:?}", e))?;

    let buffer: js_sys::ArrayBuffer = res
        .dyn_into()
        .map_err(|e| format!("Expected ArrayBuffer response, got: {:?}", e))?;

    bytes_to_blob_url(buffer)
}

/// Fetches the full media file for `source` and returns a blob object URL usable
/// directly in `src`/`href` attributes. Callers that replace or drop the URL should
/// release it with `web_sys::Url::revoke_object_url`, or the bytes stay in memory
/// for the lifetime of the page.
pub fn get_media_blob_url(source: &MediaSource) -> ArcRwSignal<Option<String>> {
    let signal = ArcRwSignal::new(None);

    let args = match serde_wasm_bindgen::to_value(&json!({ "source": source })) {
        Ok(args) => args,
        Err(e) => {
            log::error!("Failed to serialize request: {:?}", e);
            return signal;
        }
    };

    let signal_clone = signal.clone();

    spawn_local(async move {
        match fetch_blob_url("get_file", args).await {
            Ok(blob_url) => signal_clone.set(Some(blob_url)),
            Err(e) => log::error!("Failed to fetch file: {e}"),
        }
    });

    signal
}

/// Like [`get_media_blob_url`], but requests a server-generated thumbnail of the
/// given size instead of the full file. Falls back to the full file if the server
/// refuses to generate a thumbnail — some homeservers only support a fixed set of
/// preset sizes and reject arbitrary dimensions, and this keeps the image visible
/// rather than leaving the caller's signal stuck at `None` forever.
pub fn get_thumbnail_blob_url(
    source: &MediaSource,
    settings: &UiThumbnailSettings,
) -> ArcRwSignal<Option<String>> {
    let signal = ArcRwSignal::new(None);

    let args = match serde_wasm_bindgen::to_value(&json!({
        "source": source,
        "settings": settings,
    })) {
        Ok(args) => args,
        Err(e) => {
            log::error!("Failed to serialize request: {:?}", e);
            return signal;
        }
    };

    let signal_clone = signal.clone();
    let source = source.clone();

    spawn_local(async move {
        match fetch_blob_url("get_thumbnail", args).await {
            Ok(blob_url) => {
                signal_clone.set(Some(blob_url));
                return;
            }
            Err(e) => log::debug!("Thumbnail request failed, falling back to full file: {e}"),
        }

        let Ok(fallback_args) = serde_wasm_bindgen::to_value(&json!({ "source": &source })) else {
            log::error!("Failed to serialize fallback file request");
            return;
        };

        match fetch_blob_url("get_file", fallback_args).await {
            Ok(blob_url) => signal_clone.set(Some(blob_url)),
            Err(e) => log::error!("Fallback file fetch also failed: {e}"),
        }
    });

    signal
}
