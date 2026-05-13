use std::{collections::HashMap, sync::Arc};

use chrono::Local;
use ego_tree::NodeRef;
use log::{error, info, warn};
use scraper::{Html, Node};
use serde_json::json;
use shared::messages::{
    Mentions, MessageContent, MessageKind, RichTextSpan, SystemMessage, UiMessage, UserMessage
};
use tauri::{AppHandle, State, async_runtime::spawn, command};
use uuid::Uuid;

use crate::{
    frontend::emit_messages_update,
    matrix_api::messages::send_message_to_matrix,
    state::{AppState, HomeServerInfo},
    storage::messages::{delete_message, save_messages, MessageRow},
    TauriError,
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

    let txn_id = Uuid::new_v4().to_string();
    let user_id = state.user_id().await?;
    let timestamp = Local::now().timestamp() as u64;

    let message = UiMessage {
        event_id: txn_id.clone(),
        timestamp: timestamp,
        is_pending: true,
        kind: MessageKind::UserMessage(UserMessage {
            mentions: mentions.clone(),
            reactions: Vec::new(),
            replies_to: None,
            content: MessageContent::Text {
                spans: spans.clone(),
                is_edited: false,
            },
        }),
        sender_id: user_id.clone(),
    };

    let message_json = json!({
        "type": "m.room.message",
        "sender": user_id,
        "content": {
            "msgtype": "m.text",
            "body": body,
            "format": "org.matrix.custom.html",
            "formatted_body": formatted_body,
            "m.mentions": {
                "user_ids": mentions.user_ids,
                "room": mentions.room,
            }
        },
        "origin_server_ts": timestamp,
        "event_id": txn_id,
        "room_id": room_id,
    });

    let db_message = MessageRow {
        event_id: txn_id.clone(),
        room_id: room_id.clone(),
        sender: user_id,
        msg_type: "m.room.message".to_string(),
        timestamp,
        raw_json: message_json.to_string(),
    };

    state
        .with_connection_mut(move |conn| save_messages(conn, vec![db_message]))
        .await?;

    let (access_token, HomeServerInfo {base_url, supported_versions}) = state.get_api().await?;
    let txn_id_clone = txn_id.clone();
    let room_id_clone = room_id.clone();
    let state_clone = state.inner().clone();

    spawn(async move {
        let event_id = match send_message_to_matrix(
            base_url,
            &supported_versions,
            access_token,
            &room_id_clone,
            txn_id,
            body,
            formatted_body,
            mentions,
        ).await {
            Ok(res) => res,
            Err(error) => {
                error!("Failed to send message: {:?}", error);
                return;
            }
        };

        state_clone.messages_to_delete.write().await.insert(event_id, txn_id_clone.clone());

        let mut conn_guard = state_clone.connection.lock().await;
        let Some(conn) = conn_guard.as_mut() else {
            warn!("Connection not initialized, cannot delete message");
            return;
        };

        if let Err(error) = delete_message(conn, &txn_id_clone) {
            error!("Failed to delete message {}: {:?}", txn_id_clone, error);
        }
    });

    let dict = HashMap::from([(room_id.clone(), vec![message.clone()])]);
    emit_messages_update(&handle, &dict)?;

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
            // Priority: Check for data attributes first
            if let Some(data_type) = elem.attr("data-type")
                && let Some(id) = elem.attr("data-id")
            {
                let display_text = extract_text(node);

                if data_type == "room_mention" {
                    spans.push(RichTextSpan::RoomMention);
                    body.push_str("@room");
                    mentions.room = true;
                    formatted.push_str("<strong>@room</strong>");
                } else if data_type == "user_mention" {
                    // It's a User Mention
                    mentions.user_ids.push(id.to_string());
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
                    body.push_str("\n");
                    formatted.push_str("<br>");
                    spans.push(RichTextSpan::Newline);
                }
                other => warn!("Unknown element: {other}"),
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
