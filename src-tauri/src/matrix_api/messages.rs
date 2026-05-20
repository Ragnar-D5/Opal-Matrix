use matrix_sdk_crypto::EncryptionSettings;
use ruma::api::SupportedVersions;
use ruma::events::{
    AnyMessageLikeEvent, AnyStateEvent, AnyStateEventContent, AnySyncTimelineEvent,
    AnyTimelineEvent, Mentions,
};
use ruma::serde::Raw;
use ruma::RoomId;
use serde_json::{json, Value};
use shared::api::FetchMessagesResponse;
use shared::messages::{
    MessageContent, MessageKind, MessageState, Reactor, RichTextSpan, SystemMessage, UiMessage,
    UserMessage,
};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::{str::FromStr, sync::Arc};
use tauri_plugin_http::reqwest::{self, Client};

use ruma::{
    api::{
        auth_scheme::SendAccessToken,
        client::message::get_message_events::v3::{
            Request as MessageEventsRequest, Response as MessageEventsResponse,
        },
        IncomingResponse, OutgoingRequest,
    },
    OwnedRoomId, UInt,
};

use crate::matrix_api::crypto::{handle_outgoing_requests, send_post, send_to_device_request};
use crate::matrix_api::sync::extract_timeline;
use crate::storage::message_extra::{get_reactions_for_message, save_reactions, ReactionRow};
use crate::storage::messages::redact_messages;
use crate::storage::{apply_sync_changes, SyncChanges};
use crate::{
    matrix_api::crypto::process_message,
    state::HomeServerInfo,
    storage::{
        messages::{get_messages, save_messages, MessageRow},
        rooms::save_prev_token,
    },
    AppState,
};
use log::{debug, warn};
use rusqlite::{params, Connection, OptionalExtension};
use tauri::{command, State};

use crate::{reqwest_response_to_http_response, TauriError};

/// Fetches messages from the Matrix server for a given room, starting from a specified pagination token. Returns the messages and the next pagination token (if available).
async fn get_messages_api(
    room_id: &String,
    prev_batch: &String,
    server_info: &HomeServerInfo,
    access_token: &String,
    limit: usize,
) -> Result<(Vec<Raw<AnyTimelineEvent>>, Option<String>), TauriError> {
    let mut req = MessageEventsRequest::backward(OwnedRoomId::from_str(room_id.as_str())?);

    req.limit = UInt::try_from(limit)?;
    req.from = Some(prev_batch.to_string());

    let req = req.try_into_http_request::<Vec<u8>>(
        &server_info.base_url,
        SendAccessToken::Always(access_token),
        Cow::Borrowed(&server_info.supported_versions),
    )?;

    let http_req = reqwest::Request::try_from(req)?;

    let res = reqwest_response_to_http_response(Client::new().execute(http_req).await?).await?;

    let messages_res = MessageEventsResponse::try_from_http_response(res)?;

    // let mut changes = SyncChanges::default();

    // for raw_event in messages_res.chunk.into_iter() {
    //     if let Ok(event) = raw_event.deserialize() {
    //         extract_timeline(
    //             &mut changes,
    //             &ruma_room_id,
    //             event.into(),
    //             raw_event.clone().json(),
    //             raw_event.cast(),
    //         )
    //         .map_err(|e| {
    //             log::error!("Failed to extract timeline event: {:?}", e);
    //         });
    //     }
    // }

    Ok((messages_res.chunk, messages_res.end))
}

fn get_ui_messages_from_rows(
    conn: &mut Connection,
    room_id: &String,
    messages: Vec<MessageRow>,
) -> Result<Vec<UiMessage>, TauriError> {
    let mut ui_messages: Vec<UiMessage> = messages
        .into_iter()
        .filter_map(|v| v.try_into().ok())
        .collect();

    let mut reactions: HashMap<String, HashSet<ReactionRow>> = HashMap::new();
    let mut redactions: Vec<String> = Vec::new();
    for msg in ui_messages.iter() {
        if let MessageKind::SystemMessage(SystemMessage::MessageReacted { event_id, reaction }) =
            &msg.kind
        {
            reactions
                .entry(event_id.clone())
                .or_insert_with(HashSet::new)
                .insert(ReactionRow {
                    event_id: msg.event_id.clone(),
                    room_id: room_id.clone(),
                    target_event_id: event_id.clone(),
                    sender_id: msg.sender_id.clone(),
                    reaction: reaction.clone(),
                    timestamp: 0,
                });
        } else if let MessageKind::SystemMessage(SystemMessage::MessageRedacted {
            event_id, ..
        }) = &msg.kind
        {
            redactions.push(event_id.clone());
        }
    }

    ui_messages = ui_messages
        .into_iter()
        .map(|mut msg| {
            let mut old_reactions: HashSet<ReactionRow> =
                get_reactions_for_message(conn, &msg.event_id)
                    .unwrap_or_default()
                    .into_iter()
                    .collect();

            if let Some(new_reactions) = reactions.get(&msg.event_id) {
                old_reactions.extend(new_reactions.clone());
            }

            let reactions_map: HashMap<String, HashSet<Reactor>> = old_reactions.into_iter().fold(
                HashMap::new(),
                |mut acc: HashMap<String, HashSet<Reactor>>, reaction| {
                    acc.entry(reaction.reaction.clone())
                        .or_insert_with(HashSet::new)
                        .insert(Reactor {
                            user_id: reaction.sender_id.clone(),
                            event_id: reaction.event_id.clone(),
                        });
                    acc
                },
            );

            msg.set_reactions(&reactions_map);

            msg
        })
        .collect();

    save_reactions(conn, reactions.into_values().flatten().collect())?;
    redact_messages(conn, redactions)?;

    Ok(ui_messages)
}

#[command(rename_all = "snake_case")]
pub async fn fetch_messages(
    state: State<'_, Arc<AppState>>,
    room_id: String,
    oldest_id: Option<String>,
) -> Result<FetchMessagesResponse, TauriError> {
    log::debug!(
        "Fetching messages for room {}, starting from oldest_id: {:?}",
        room_id,
        oldest_id
    );
    let limit = 20;

    let mut conn_guard = state.connection.lock().await;
    let conn = conn_guard
        .as_mut()
        .ok_or("Database connection not available")?;

    let mut local_messages = get_messages(&conn, &room_id, oldest_id.clone(), limit)?;

    if local_messages.len() >= limit {
        return Ok(FetchMessagesResponse {
            messages: get_ui_messages_from_rows(conn, &room_id, local_messages)?,
            has_more: true,
        });
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
        return Ok(FetchMessagesResponse {
            messages: get_ui_messages_from_rows(conn, &room_id, local_messages)?,
            has_more: false,
        });
    };

    let (access_token, server_info) = state.get_api().await?;

    let (api_messages, next_token) =
        get_messages_api(&room_id, &prev_token, &server_info, &access_token, limit).await?;

    let mut changes = SyncChanges::default();

    let ruma_room_id = RoomId::parse(room_id)?;

    let mut hit_room_create = false;

    for raw_event in api_messages.into_iter() {
        let decrypted_event =
            if let Ok(AnyTimelineEvent::MessageLike(AnyMessageLikeEvent::RoomEncrypted(ev))) =
                &raw_event.deserialize()
            {
                match process_message(
                    &state,
                    &room_id,
                    Raw::from_json(raw_event.clone().into_json()),
                )
                .await
                {
                    Ok(processed) => processed,
                    Err(e) => {
                        log::error!("Failed to process encrypted message: {:?}", e);
                        continue;
                    }
                }
            } else {
                raw_event.clone()
            };

        if let Ok(event) = decrypted_event.deserialize() {
            if let AnyTimelineEvent::State(AnyStateEvent::RoomCreate(_)) = &event {
                hit_room_create = true;
            }

            extract_timeline(
                &mut changes,
                &ruma_room_id,
                event.into(),
                decrypted_event.json(),
                decrypted_event.cast(),
            )
            .map_err(|e| {
                log::error!("Failed to extract timeline event: {:?}", e);
            });
        }
    }

    let api_messages = changes.new_messages;
    apply_sync_changes(conn, changes)?;

    if let Some(next_token) = next_token.clone() {
        save_prev_token(conn, &room_id, &next_token)?;
    }

    let has_more = !hit_room_create && next_token.is_some() && next_token != Some(prev_token);

    Ok(FetchMessagesResponse {
        messages: get_ui_messages_from_rows(conn, &room_id, local_messages)?,
        has_more,
    })
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
                    state: MessageState::Sent,
                    is_edited: false,
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

use ruma::api::client::message::send_message_event::v3::{
    Request as SendMessageRequest, Response as SendMessageResponse,
};

/// This function is called to send the contents of the input field
/// as m.room.message
// pub async fn send_message_to_matrix(
//     base_url: String,
//     supported_versions: &SupportedVersions,
//     access_token: String,
//     room_id: &String,
//     txn_id: String,
//     body: String,
//     formatted_body: String,
//     mentions: Mentions,
// ) -> Result<String, TauriError> {
//     let message_content =
//         ruma::events::room::message::RoomMessageEventContent::text_html(body, formatted_body)
//             .add_mentions(mentions);

//     let ruma_request = SendMessageRequest::new(
//         room_id.clone().try_into()?,
//         txn_id.clone().into(),
//         &message_content,
//     )?;

//     let http_request = ruma_request.try_into_http_request::<Vec<u8>>(
//         base_url.as_str(),
//         SendAccessToken::IfRequired(access_token.as_str()),
//         Cow::Borrowed(&supported_versions),
//     )?;

//     let reqwest_request = reqwest::Request::try_from(http_request.clone())?;

//     let client = reqwest::Client::new();

//     let mut response = client.execute(reqwest_request).await?;
//     let mut timeout = 1;

//     while !response.status().is_success() {
//         if timeout >= 120 {
//             return Err("Failed to send message after timeout was reached".into());
//         }

//         timeout *= 2;
//         tokio::time::sleep(std::time::Duration::from_secs(timeout)).await;

//         response = client
//             .execute(reqwest::Request::try_from(http_request.clone())?)
//             .await?;
//     }

//     let http_res = reqwest_response_to_http_response(response).await?;

//     let res = SendMessageResponse::try_from_http_response(http_res)?;

//     Ok(res.event_id.to_string())
// }

/// This function is called to send the contents of the input field
/// as m.room.message
///
/// This will also share new room keys if neccessary and after that
/// encrypt and send the message
pub async fn send_message_to_matrix(
    base_url: String,
    supported_versions: &SupportedVersions,
    access_token: String,
    room_id: &String,
    txn_id: String,
    body: String,
    formatted_body: String,
    mentions: Mentions,
    state: Arc<AppState>,
    members: Vec<String>,
    algorithm: Option<String>,
) -> Result<String, TauriError> {
    let client = reqwest::Client::new();

    let message_content =
        ruma::events::room::message::RoomMessageEventContent::text_html(body, formatted_body)
            .add_mentions(mentions);

    let http_request = if algorithm.is_some() {
        let crypto_machine = {
            let mut crypto_guard = state.crypto_machine.lock().await;
            crypto_guard
                .as_mut()
                .cloned()
                .ok_or("Crypto machine not initialized")?
        };

        let parsed_user_ids: Vec<ruma::OwnedUserId> = members
            .into_iter()
            .map(|string| ruma::UserId::parse(string))
            .collect::<Result<Vec<_>, _>>()?;

        let user_id_refs = parsed_user_ids.iter().map(AsRef::as_ref);

        crypto_machine
            .update_tracked_users(user_id_refs.clone())
            .await?;

        if let Some((txn_id, keys_claim_request)) = crypto_machine
            .get_missing_sessions(user_id_refs.clone())
            .await?
        {
            debug!("Missing sessions found, claiming keys...");

            let url = format!("{}/_matrix/client/v3/keys/claim", base_url);

            let body = json!({
                "one_time_keys": keys_claim_request.one_time_keys,
            });

            let http_res = send_post(&client, url, body, &access_token).await?;

            let matrix_res =
                ruma::api::client::keys::claim_keys::v3::Response::try_from_http_response(
                    http_res,
                )?;

            crypto_machine
                .mark_request_as_sent(&txn_id, &matrix_res)
                .await?;
        }

        debug!("Trying to share room keys");
        let requests = crypto_machine
            .share_room_key(
                &RoomId::parse(room_id)?,
                user_id_refs,
                EncryptionSettings::default(),
            )
            .await?;
        handle_outgoing_requests(&crypto_machine, &access_token, &base_url).await?;

        for request in requests {
            let matrix_response =
                send_to_device_request(&base_url, &request, &client, &access_token).await?;

            crypto_machine
                .mark_request_as_sent(&request.txn_id, &matrix_response)
                .await?;
        }

        let encrypted_content_intermediate = serde_json::to_value(
            crypto_machine
                .encrypt_room_event(
                    room_id.as_str().try_into().unwrap(),
                    message_content.clone(),
                )
                .await?
                .deserialize()?
                .scheme,
        )?;

        use ruma::events::room::encrypted::*;

        let encrypted_content: MegolmV1AesSha2Content =
            serde_json::from_value(encrypted_content_intermediate)?;

        let room_encrypted_event_content = RoomEncryptedEventContent::new(
            EncryptedEventScheme::MegolmV1AesSha2(encrypted_content),
            None,
        ); //change None to accomodate relation of messages

        let ruma_request = SendMessageRequest::new(
            room_id.clone().try_into()?,
            txn_id.clone().into(),
            &room_encrypted_event_content,
        )?;

        ruma_request.try_into_http_request::<Vec<u8>>(
            base_url.as_str(),
            SendAccessToken::IfRequired(access_token.as_str()),
            Cow::Borrowed(&supported_versions),
        )?
    } else {
        let ruma_request = SendMessageRequest::new(
            room_id.clone().try_into()?,
            txn_id.clone().into(),
            &message_content,
        )?;

        ruma_request.try_into_http_request::<Vec<u8>>(
            base_url.as_str(),
            SendAccessToken::IfRequired(access_token.as_str()),
            Cow::Borrowed(&supported_versions),
        )?
    };

    let reqwest_request = reqwest::Request::try_from(http_request.clone())?;

    let mut response = client.execute(reqwest_request).await?;
    let mut timeout = 1;

    while !response.status().is_success() {
        if timeout >= 120 {
            return Err("Failed to send message after timeout was reached".into());
        }

        timeout *= 2;
        tokio::time::sleep(std::time::Duration::from_secs(timeout)).await;

        response = client
            .execute(reqwest::Request::try_from(http_request.clone())?)
            .await?;
    }

    let http_res = reqwest_response_to_http_response(response).await?;

    let res = SendMessageResponse::try_from_http_response(http_res)?;

    Ok(res.event_id.to_string())
}
