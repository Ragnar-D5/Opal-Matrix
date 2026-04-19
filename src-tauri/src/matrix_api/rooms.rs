use log::debug;
use std::{collections::HashSet, sync::Arc};

use crate::{
    storage::{
        messages::{get_messages, save_messages, MessageRow},
        rooms::save_prev_token,
    },
    AppState,
};
use log::warn;
use reqwest::Client;
use rusqlite::{params, OptionalExtension};
use tauri::{command, State};

use crate::{construct_url, frontend::messages::Message, TauriError};

async fn get_messages_api(
    room_id: &String,
    prev_batch: &String,
    matrix_url: &String,
    access_token: &String,
    limit: usize,
) -> Result<(Vec<MessageRow>, Option<String>), TauriError> {
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
            let messages = chunk
                .iter()
                .filter_map(|event| {
                    if let (Some(event_id), Some(msg_type), Some(sender), Some(ts)) = (
                        event.get("event_id").and_then(|e| e.as_str()),
                        event.get("type").and_then(|t| t.as_str()),
                        event.get("sender").and_then(|s| s.as_str()),
                        event.get("origin_server_ts").and_then(|t| t.as_i64()),
                    ) {
                        Some(MessageRow {
                            event_id: event_id.to_string(),
                            room_id: room_id.clone(),
                            sender: sender.to_string(),
                            msg_type: msg_type.to_string(),
                            raw_json: event.to_string(),
                            timestamp: ts / 1000,
                        })
                    } else {
                        None
                    }
                })
                .collect();

            return Ok((messages, next_batch));
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
) -> Result<(Vec<Message>, bool), TauriError> {
    debug!(
        "Fetching messages for room_id: {}, oldest_id: {:?}",
        room_id, oldest_id
    );
    let limit = 20;

    let mut conn_guard = state.connection.lock().await;
    let conn = conn_guard
        .as_mut()
        .ok_or("Database connection not available")?;

    let mut local_messages = get_messages(&conn, &room_id, oldest_id.clone(), limit)?;

    if local_messages.len() >= limit {
        return Ok((local_messages.into_iter().map(|m| m.into()).collect(), true));
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
            local_messages.into_iter().map(|m| m.into()).collect(),
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

    save_messages(conn, api_messages.clone())?;

    if let Some(next_token) = next_token {
        save_prev_token(conn, &room_id, &next_token)?;
    }

    let mut seen_ids: HashSet<String> = local_messages.iter().map(|m| m.event_id.clone()).collect();
    for msg in api_messages {
        if !seen_ids.contains(&msg.event_id) {
            local_messages.push(msg.clone());
            seen_ids.insert(msg.event_id.clone());
        }
    }

    let has_moe = local_messages.len() >= limit;

    Ok((
        local_messages.into_iter().map(|m| m.into()).collect(),
        has_moe,
    ))
}
