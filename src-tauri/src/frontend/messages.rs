use ruma::api::{OutgoingRequest, auth_scheme::SendAccessToken};
use std::{borrow::Cow, collections::HashMap, sync::Arc};
use tauri_plugin_http::reqwest;

use chrono::Local;
use ego_tree::NodeRef;
use log::{info, warn};
use scraper::{Html, Node};
use serde_json::json;
use shared::messages::{
    Mentions, MessageContent, MessageKind, RichTextSpan, UiMessage, UserMessage,
};
use tauri::{AppHandle, State, command};
use uuid::Uuid;

use crate::{
    TauriError,
    frontend::send_messages_update,
    state::AppState,
    storage::messages::{MessageRow, save_messages},
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

    let dict = HashMap::from([(room_id.clone(), vec![message.clone()])]);

    send_messages_update(&handle, dict)?;

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
        "event_id": txn_id.clone(),
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

    let message_content =
        ruma::events::room::message::RoomMessageEventContent::text_html(body, formatted_body);

    let ruma_request = ruma::api::client::message::send_message_event::v3::Request::new(
        room_id.clone().try_into()?,
        txn_id.clone().into(),
        &message_content,
    )?;

    let info = state.get_api().await?;

    let http_request = ruma_request.try_into_http_request::<Vec<u8>>(
        &info.1.base_url,
        SendAccessToken::IfRequired(&info.0),
        Cow::Borrowed(&info.1.supported_versions),
    )?;

    warn!("{:?}", http_request.body());

    let reqwest_request = reqwest::Request::try_from(http_request.clone())?;

    let client = reqwest::Client::new();

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

    info!("{:?}", response.status());
    info!("Successfully sent message");
    // TODO: Change message in db?

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
