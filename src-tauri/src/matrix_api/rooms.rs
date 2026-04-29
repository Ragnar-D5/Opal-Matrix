use log::debug;
use serde_json::Value;
use shared::messages::UiMessage;
use std::sync::Arc;
use tauri_plugin_http::reqwest::Client;

use crate::{
    AppState,
    matrix_api::crypto::process_message,
    storage::{
        messages::{MessageRow, get_messages, save_messages},
        rooms::save_prev_token,
    },
};
use log::warn;
use rusqlite::{OptionalExtension, params};
use tauri::{State, command};

use crate::{TauriError, construct_url};

async fn get_messages_api(
    room_id: &String,
    prev_batch: &String,
    matrix_url: &String,
    access_token: &String,
    limit: usize,
) -> Result<(Vec<Value>, Option<String>), TauriError> {
    let client = Client::new();

    let url = construct_url(vec![
        matrix_url,
        "_matrix",
        "client",
        "v3",
        "rooms",
        room_id,
        format!("messages?from={prev_batch}&dir=b&limit={limit}").as_str(),
    ])?;

    let res = client.get(url).bearer_auth(access_token).send().await?;

    if res.status().is_success() {
        let res_json = res.json::<serde_json::Value>().await?;

        let next_batch = res_json
            .get("end")
            .and_then(|b| b.as_str())
            .map(|s| s.to_string());

        if let Some(chunk) = res_json.get("chunk").and_then(|c| c.as_array()) {
            return Ok((chunk.clone(), next_batch));
        } else {
            return Err("Malformed response: missing 'chunk' array".into());
        }
    } else {
        return Err(format!(
            "Failed to fetch messages: HTTP {}: {}",
            res.status(),
            res.text()
                .await
                .unwrap_or_else(|_| "No response body".to_string())
        )
        .into());
    }
}

#[command(rename_all = "snake_case")]
pub async fn fetch_messages(
    state: State<'_, Arc<AppState>>,
    room_id: String,
    oldest_id: Option<String>,
) -> Result<(Vec<UiMessage>, bool), TauriError> {
    let limit = 20;

    let mut conn_guard = state.connection.lock().await;
    let conn = conn_guard
        .as_mut()
        .ok_or("Database connection not available")?;

    let mut local_messages = get_messages(&conn, &room_id, oldest_id.clone(), limit)?;

    if local_messages.len() >= limit {
        return Ok((
            local_messages
                .into_iter()
                .filter_map(|m| m.try_into().ok())
                .collect(),
            true,
        ));
    }

    let prev_batch: Option<String> = conn
        .query_row(
            "SELECT prev_batch FROM rooms WHERE room_id = ?",
            params![room_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();

    let Some(prev_token) = prev_batch else {
        warn!("Room {room_id} has no prev_batch token, cannot fetch more messages from server");
        return Ok((
            local_messages
                .into_iter()
                .filter_map(|m| m.try_into().ok())
                .collect(),
            false,
        ));
    };

    let access_token = {
        let guard = state.token.read().await;
        guard
            .clone()
            .ok_or("Access token not available")?
            .access_token
    };
    let matrix_url = {
        let guard = state.matrix_url.read().await;
        guard.clone().ok_or("Matrix URL not available")?
    };

    let (api_messages, next_token) =
        get_messages_api(&room_id, &prev_token, &matrix_url, &access_token, limit).await?;

    if let Some(next_token) = next_token {
        save_prev_token(conn, &room_id, &next_token)?;
    }

    for msg in api_messages {
        if msg
            .to_string()
            .contains("$cse5M93hIaL9xnDvUJ93MNIIb6LRw6dluFAuowNWiGI")
        {
            debug!("Got message: {msg}");
        }

        let Some(event_id) = msg
            .get("event_id")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
        else {
            continue;
        };
        let Some(msg_type) = msg
            .get("type")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
        else {
            continue;
        };

        let msg = if msg_type == "m.room.encrypted" {
            match process_message(&state, &room_id, msg).await {
                Ok(res) => res,
                Err(_) => continue,
            }
        } else {
            msg
        };
        let Some(msg_type) = msg
            .get("type")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
        else {
            continue;
        };

        let Some(timestamp) = msg.get("origin_server_ts").and_then(|v| v.as_i64()) else {
            continue;
        };
        let Some(sender) = msg.get("sender").and_then(|v| v.as_str()) else {
            continue;
        };
        local_messages.push(MessageRow {
            event_id: event_id.to_string(),
            room_id: room_id.clone(),
            sender: sender.to_string(),
            msg_type: msg_type.to_string(),
            raw_json: msg.to_string(),
            timestamp: timestamp / 1000,
        });
    }

    save_messages(conn, local_messages.clone())?;

    let has_moe = local_messages.len() >= limit;

    Ok((
        local_messages
            .into_iter()
            .filter_map(|m| m.try_into().ok())
            .collect(),
        has_moe,
    ))
}
