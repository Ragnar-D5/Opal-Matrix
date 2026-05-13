use log::debug;
use serde_json::Value;
use shared::messages::UiMessage;
use std::borrow::Cow;
use std::{str::FromStr, sync::Arc};
use tauri_plugin_http::reqwest::{self, Client};

use ruma::{
    OwnedRoomId, UInt,
    api::{
        IncomingResponse, OutgoingRequest,
        auth_scheme::SendAccessToken,
        client::message::get_message_events::v3::{
            Request as MessageEventsRequest, Response as MessageEventsResponse,
        },
    },
};

use crate::{
    AppState,
    matrix_api::crypto::process_message,
    state::HomeServerInfo,
    storage::{
        messages::{MessageRow, get_messages, save_messages},
        rooms::save_prev_token,
    },
};
use log::warn;
use rusqlite::{OptionalExtension, params};
use tauri::{State, command};

use crate::{TauriError, reqwest_response_to_http_response};

/// Fetches messages from the Matrix server for a given room, starting from a specified pagination token. Returns the messages and the next pagination token (if available).
async fn get_messages_api(
    room_id: &String,
    prev_batch: &String,
    server_info: &HomeServerInfo,
    access_token: &String,
    limit: usize,
) -> Result<(Vec<Value>, Option<String>), TauriError> {
    let mut req = MessageEventsRequest::backward(OwnedRoomId::from_str(room_id.as_str())?);

    req.limit = UInt::try_from(limit)?;
    req.from = Some(prev_batch.to_string());

    let req = req.try_into_http_request::<Vec<u8>>(
        &server_info.base_url,
        SendAccessToken::Always(access_token),
        Cow::Owned(server_info.supported_versions.clone()),
    )?;

    let http_req = reqwest::Request::try_from(req)?;

    let res = reqwest_response_to_http_response(Client::new().execute(http_req).await?).await?;

    let messages_res = MessageEventsResponse::try_from_http_response(res)?;

    Ok((
        messages_res
            .chunk
            .iter()
            .filter_map(|v| serde_json::from_str::<Value>(v.json().get()).ok())
            .collect(),
        messages_res.end,
    ))
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

    let (access_token, server_info) = state.get_api().await?;

    let (api_messages, next_token) =
        get_messages_api(&room_id, &prev_token, &server_info, &access_token, limit).await?;

    if let Some(next_token) = next_token.clone() {
        save_prev_token(conn, &room_id, &next_token)?;
    }

    let mut hit_room_create = false;

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

        if msg_type == "m.room.create" {
            hit_room_create = true;
        }

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
            timestamp: timestamp as u64 / 1000,
        });
    }

    save_messages(conn, local_messages.clone())?;

    let has_more = !hit_room_create && next_token.is_some() && next_token != Some(prev_token);

    Ok((
        local_messages
            .into_iter()
            .filter_map(|m| m.try_into().ok())
            .collect(),
        has_more,
    ))
}

pub async fn backfill_gap(
    state: &Arc<AppState>,
    room_id: String,
    start_token: String,
) -> Result<(), TauriError> {
    let mut current_token = start_token;

    let (access_token, server_info) = state.get_api().await?;

    loop {
        let (api_messages, next_token) =
            get_messages_api(&room_id, &current_token, &server_info, &access_token, 50).await?;

        if api_messages.is_empty() {
            break;
        }

        let mut new_rows = Vec::new();
        let mut hit_existing = false;

        {
            let mut conn_guard = state.connection.lock().await;
            let conn = conn_guard.as_mut().ok_or("Database not initialized")?;

            for msg_val in api_messages {
                let clone = msg_val.clone();
                let Some(event_id) = clone.get("event_id").and_then(|v| v.as_str()) else {
                    continue;
                };

                if crate::storage::messages::message_exists(conn, event_id)? {
                    hit_existing = true;
                    break;
                }

                let Some(msg_type) = msg_val.get("type").and_then(|v| v.as_str()) else {
                    continue;
                };

                let processed_msg = if msg_type == "m.room.encrypted" {
                    match process_message(state, &room_id, msg_val.clone()).await {
                        Ok(res) => res,
                        Err(_) => continue,
                    }
                } else {
                    msg_val
                };

                let Some(final_type) = processed_msg.get("type").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(timestamp) = processed_msg
                    .get("origin_server_ts")
                    .and_then(|v| v.as_i64())
                else {
                    continue;
                };
                let Some(sender) = processed_msg.get("sender").and_then(|v| v.as_str()) else {
                    continue;
                };

                new_rows.push(MessageRow {
                    event_id: event_id.to_string(),
                    room_id: room_id.clone(),
                    sender: sender.to_string(),
                    msg_type: final_type.to_string(),
                    raw_json: processed_msg.to_string(),
                    timestamp: timestamp as u64 / 1000,
                });
            }

            if !new_rows.is_empty() {
                save_messages(conn, new_rows)?;
            }
        }

        if hit_existing {
            break;
        }

        if let Some(token) = next_token {
            current_token = token;
        } else {
            break;
        }
    }

    Ok(())
}
