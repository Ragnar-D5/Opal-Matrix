use futures::StreamExt;
use matrix_sdk::Client as MatrixClient;
use std::str::FromStr;
use tokio_util::sync::CancellationToken;

use ego_tree::NodeRef;
use log::{error, warn};
use ruma::{
    OwnedUserId, RoomId,
    events::{AnyMessageLikeEventContent, Mentions, room::message::RoomMessageEventContent},
};
use scraper::{Html, Node};
use shared::timeline::{RichTextSpan, UiTimelineDiff, UiTimelineItem};
use tauri::{AppHandle, Emitter, State, command};
use tokio::sync::RwLock;

use crate::{
    TauriError,
    state::{TaskManager, TimelineManager},
};

#[command(rename_all = "snake_case")]
pub async fn commit_message(
    html: String,
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    room_id: String,
) -> Result<(), TauriError> {
    let client = matrix_client.read().await;
    let room = client
        .get_room(&RoomId::parse(&room_id)?)
        .ok_or("Room not found")?;

    let timeline = timeline_manager.get_or_create_timeline(&room).await?;

    let mut mentions = Mentions::default();
    let mut spans = Vec::new();

    let (body, formatted_body) = process_string_to_message(&html, &mut mentions, &mut spans);

    let mut message_content = RoomMessageEventContent::text_html(body, formatted_body);
    message_content.mentions = Some(mentions.clone());

    let content = AnyMessageLikeEventContent::RoomMessage(message_content);
    timeline.send(content).await?;

    Ok(())
}

fn process_string_to_message(
    html: &str,
    mentions: &mut Mentions,
    spans: &mut Vec<RichTextSpan>,
) -> (String, String) {
    let fragment = Html::parse_fragment(html);

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
pub async fn scroll_up(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    room_id: String,
) -> Result<bool, TauriError> {
    log::debug!("Scrolling up");
    let room = matrix_client
        .read()
        .await
        .get_room(&RoomId::parse(&room_id)?)
        .ok_or("No room found")?;

    let timeline_manager = timeline_manager.inner();
    let timeline = timeline_manager.get_or_create_timeline(&room).await?;

    let has_more = timeline.paginate_backwards(30).await?;

    Ok(!has_more)
}

#[command(rename_all = "snake_case")]
pub async fn get_timeline(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    task_manager: State<'_, TaskManager>,
    handle: AppHandle,
    room_id: String,
) -> Result<Vec<UiTimelineItem>, TauriError> {
    log::debug!("Fetching timeline for room {}", room_id);

    let token = CancellationToken::new();
    task_manager
        .replace_task("get_timeline", token.clone())
        .await;

    tokio::select! {
        _ = token.cancelled() => {
            log::debug!("Timeline fetch for room {} was cancelled by a newer request", room_id);
            Err(TauriError::from("Cancelled by newer request"))
        }

        result = async {
            log::debug!("Fetching timeline for room {}", room_id);

            let room = matrix_client
                .read()
                .await
                .get_room(&RoomId::parse(&room_id)?)
                .ok_or("No room found")?;

            timeline_manager.abort_stream().await;

            let timeline = timeline_manager.get_or_create_timeline(&room).await?;

            let (messages, stream) = timeline.subscribe().await;

            timeline_manager
                .set_stream_handle(tokio::spawn(async move {
                    tokio::pin!(stream);

                    while let Some(update) = stream.next().await {
                        send_timeline_diffs(handle.clone(), update.iter().map(|d| d.into()).collect())
                            .await;
                    }
                }))
                .await;

            log::debug!("Fetched {} messages for room {}", messages.len(), room_id);

            Ok(messages.iter().map(|v| v.into()).collect())
        } => {
            result
        }
    }
}

async fn send_timeline_diffs(handle: AppHandle, diffs: Vec<UiTimelineDiff>) {
    log::debug!("Emitting timeline update with {} diffs", diffs.len());
    if let Err(e) = handle.emit("timeline_update", diffs) {
        error!("Failed to emit timeline update: {:?}", e);
    }
}
