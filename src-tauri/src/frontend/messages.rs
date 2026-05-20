use matrix_sdk::Client as MatrixClient;
use std::{collections::HashMap, str::FromStr, sync::Arc};

use chrono::Local;
use ego_tree::NodeRef;
use log::{error, warn};
use ruma::{OwnedUserId, RoomId, events::Mentions};
use scraper::{Html, Node};
use serde_json::json;
use shared::{
    api::FetchMessagesResponse,
    messages::{MessageContent, MessageKind, MessageState, RichTextSpan, UiMessage, UserMessage},
};
use tauri::{AppHandle, State, async_runtime::spawn, command};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    TauriError,
    frontend::emit_single_message_update,
    state::{AppState, HomeServerInfo, TimelineManager},
    storage::{
        members::get_members_for_room_api,
        messages::{MessageRow, delete_message, save_messages, set_message_state},
        rooms::get_room_encryption,
    },
};

#[command(rename_all = "snake_case")]
pub async fn commit_message(
    state: State<'_, Arc<AppState>>,
    handle: AppHandle,
    html: String,
    room_id: String,
) -> Result<(), TauriError> {
    let mut mentions = Mentions::default();
    let mut spans = Vec::new();

    let (body, formatted_body) = process_string_to_message(&html, &mut mentions, &mut spans);

    let txn_id = format!("${}", Uuid::new_v4());
    let user_id = state.user_id().await?;
    let timestamp = Local::now().timestamp() as u64;

    // let message = UiMessage {
    //     event_id: txn_id.clone(),
    //     room_id: room_id.clone(),
    //     timestamp,
    //     state: MessageState::Pending,
    //     kind: MessageKind::UserMessage(UserMessage {
    //         mentions: mentions.clone(),
    //         reactions: HashMap::new(),
    //         replies_to: None,
    //         is_edited: false,
    //         content: MessageContent::Text {
    //             spans: spans.clone(),
    //         },
    //     }),
    //     sender_id: user_id.clone(),
    // };

    // let message_json = json!({
    //     "type": "m.room.message",
    //     "sender": user_id,
    //     "content": {
    //         "msgtype": "m.text",
    //         "body": body,
    //         "format": "org.matrix.custom.html",
    //         "formatted_body": formatted_body,
    //         "m.mentions": {
    //             "user_ids": mentions.user_ids,
    //             "room": mentions.room,
    //         }
    //     },
    //     "origin_server_ts": timestamp,
    //     "event_id": txn_id,
    //     "room_id": room_id,
    // });

    // let db_message = MessageRow {
    //     event_id: txn_id.clone(),
    //     room_id: room_id.clone(),
    //     sender: user_id,
    //     msg_type: "m.room.message".to_string(),
    //     timestamp,
    //     raw_json: message_json.to_string(),
    //     state: MessageState::Pending,
    //     last_edited_id: None,
    // };

    // let room_id_clone = room_id.clone();
    // let (members, algorithm) = state
    //     .with_connection_mut(move |conn| {
    //         save_messages(conn, vec![db_message])?;
    //         let members: Vec<String> = get_members_for_room_api(conn, &room_id_clone)?
    //             .into_iter()
    //             .map(|entry| entry.0)
    //             .collect();
    //         let algorithm = get_room_encryption(conn, &room_id_clone)?;

    //         Ok((members, algorithm))
    //     })
    //     .await?;

    // let (
    //     access_token,
    //     HomeServerInfo {
    //         base_url,
    //         supported_versions,
    //     },
    // ) = state.get_api().await?;
    // let txn_id_clone = txn_id.clone();
    // let room_id_clone = room_id.clone();
    // let state_clone = state.inner().clone();
    // let mut message_clone = message.clone();
    // let handle_clone = handle.clone();

    // spawn(async move {
    //     let event_id = send_message_to_matrix(
    //         base_url,
    //         &supported_versions,
    //         access_token,
    //         &room_id_clone,
    //         txn_id,
    //         body,
    //         formatted_body,
    //         mentions,
    //         state_clone.clone(),
    //         members,
    //         algorithm,
    //     )
    //     .await
    //     .map_err(|e| error!("Failed to send message: {:?}", e))
    //     .ok();

    //     let mut conn_guard = state_clone.connection.lock().await;
    //     let Some(conn) = conn_guard.as_mut() else {
    //         warn!("Connection not initialized, cannot delete message");
    //         return;
    //     };

    //     if let Some(event_id) = event_id {
    //         state_clone
    //             .messages_to_delete
    //             .write()
    //             .await
    //             .insert(event_id, txn_id_clone.clone());

    //         if let Err(error) = delete_message(conn, &txn_id_clone) {
    //             error!("Failed to delete message {}: {:?}", txn_id_clone, error);
    //         }
    //     } else {
    //         if let Err(error) = set_message_state(conn, &txn_id_clone, MessageState::Failed) {
    //             error!(
    //                 "Failed to update message state for {}: {:?}",
    //                 txn_id_clone, error
    //             );
    //         }

    //         message_clone.state = MessageState::Failed;

    //         emit_single_message_update(&handle_clone, &room_id_clone, &message_clone)
    //             .unwrap_or_else(|error| {
    //                 error!(
    //                     "Failed to emit message update for {}: {:?}",
    //                     txn_id_clone, error
    //                 );
    //             });
    //     }
    // });

    // emit_single_message_update(&handle, &room_id, &message)?;

    Ok(())
}

fn process_string_to_message(
    html: &String,
    mentions: &mut Mentions,
    spans: &mut Vec<RichTextSpan>,
) -> (String, String) {
    let fragment = Html::parse_fragment(&html);

    let mut body = String::new();
    let mut formatted_body = String::new();

    for node in fragment.tree.root().children() {
        walk_node(node, spans, mentions, &mut body, &mut formatted_body);
    }

    (body, formatted_body)
}

fn walk_node(
    node: NodeRef<'_, Node>,
    spans: &mut Vec<RichTextSpan>,
    mentions: &mut Mentions,
    body: &mut String,
    formatted: &mut String,
) {
    match node.value() {
        Node::Text(text) => {
            let content = text.text.replace("\u{a0}", " ");
            body.push_str(&content);
            formatted.push_str(&content);
            spans.push(RichTextSpan::Plain(content.to_string()));
        }
        Node::Element(elem) => {
            if let Some(url) = elem.attr("data-url") {
                let display_text = extract_text(node);
                body.push_str(&display_text);
                formatted.push_str(&format!("<a href=\"{}\">{}</a>", url, display_text));

                spans.push(RichTextSpan::Link {
                    url: url.to_string(),
                    text: Some(display_text),
                });
                return;
            }
            if let Some(data_type) = elem.attr("data-type")
                && let Some(id) = elem.attr("data-id")
            {
                let display_text = extract_text(node).trim_start_matches('@').to_string();

                if data_type == "room_mention" {
                    spans.push(RichTextSpan::RoomMention);
                    body.push_str("@room");
                    mentions.room = true;
                    formatted.push_str("<strong>@room</strong>");
                } else if data_type == "user_mention" {
                    if let Ok(user_id) = OwnedUserId::from_str(id) {
                        mentions.user_ids.insert(user_id);
                    } else {
                        warn!("Invalid user ID in mention: {id}");
                    }

                    spans.push(RichTextSpan::UserMention {
                        user_id: id.to_string(),
                        display_name: display_text.clone(),
                    });
                    body.push_str(&display_text);
                    formatted.push_str(&format!(
                        "<a href=\"https://matrix.to/#/{}\">{}</a>",
                        id, display_text
                    ));
                }
                return;
            }

            match elem.name() {
                "html" | "body" => {
                    for child in node.children() {
                        walk_node(child, spans, mentions, body, formatted);
                    }
                }
                "br" => {
                    body.push('\n');
                    formatted.push_str("<br>");
                    spans.push(RichTextSpan::Newline);
                }
                other => warn!("Unknown element: {other}; {:?}", elem),
            }
        }
        _ => {}
    }
}

fn extract_text(node: NodeRef<'_, Node>) -> String {
    let mut text = String::new();
    for child in node.children() {
        if let Node::Text(t) = child.value() {
            text.push_str(&t.text);
        } else {
            text.push_str(&extract_text(child));
        }
    }
    text
}

#[command(rename_all = "snake_case")]
pub async fn fetch_messages(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    room_id: String,
    oldest_id: Option<String>,
) -> Result<FetchMessagesResponse, TauriError> {
    let matrix_client = matrix_client.read().await;
    let room = matrix_client
        .get_room(&RoomId::parse(&room_id)?)
        .ok_or("No room found")?;

    let timeline = timeline_manager.get_or_create_timeline(&room).await?;
    timeline.paginate_backwards(30).await?;

    let (messages, _) = timeline.subscribe().await;

    log::info!("Room_id {room_id}, messages: {:?}", messages);

    let resp = FetchMessagesResponse {
        has_more: false,
        messages: Vec::new(),
    };

    Ok(resp)
}
