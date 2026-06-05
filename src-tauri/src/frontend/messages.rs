use std::{collections::HashMap, io::Cursor, path::PathBuf, str::FromStr};

use futures::{StreamExt};
use image::ImageReader;
use matrix_sdk::{Client as MatrixClient, attachment::{AttachmentInfo, BaseFileInfo, BaseImageInfo, BaseVideoInfo}, room::edit::EditedContent, ruma::{api::client::receipt::create_receipt::v3::ReceiptType, events::room::MediaSource}, };
use matrix_sdk_ui::timeline::{AttachmentConfig, AttachmentSource, TimelineEventItemId};
use mime::Mime;
use tokio_util::sync::CancellationToken;

use ego_tree::NodeRef;
use log::{error, warn};
use matrix_sdk::ruma::{
    OwnedEventId, OwnedUserId, RoomId,
    events::{
        AnyMessageLikeEventContent, Mentions,
        room::message::{RoomMessageEventContent, RoomMessageEventContentWithoutRelation},
    },
};
use scraper::{Html, Node};
use shared::{api::{FileMetadata, UiAttachmentSource}, timeline::{RichTextSpan, RoomIdFormat, UiTimelineDiff, UiTimelineItem, coalesce_diffs}};
use tauri::{AppHandle, Emitter, State, command};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    TauriError,
    frontend::timeline::{timeline_diff_to_ui, timeline_item_to_ui},
    state::{MediaManager, TaskManager, TimelineManager},
};

#[command(rename_all = "snake_case")]
pub async fn commit_message(
    html: String,
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    room_id: String,
    replies_to: Option<String>,
) -> Result<(), TauriError> {
    log::debug!("Committing message to room {}", room_id);
    let client = matrix_client.read().await;
    let room = client
        .get_room(&RoomId::parse(&room_id)?)
        .ok_or("Room not found")?;

    let timeline = timeline_manager.get_or_create_timeline(&room).await?;

    let mut mentions = Mentions::default();
    let mut spans = Vec::new();

    let (body, formatted_body) = process_string_to_message(&html, &mut mentions, &mut spans);

    if let Some(reply_to_id) = replies_to {
        let content = RoomMessageEventContentWithoutRelation::text_html(body, formatted_body);
        timeline
            .send_reply(content, OwnedEventId::try_from(reply_to_id)?)
            .await?;
    } else {
        let mut message_content = RoomMessageEventContent::text_html(body, formatted_body);
        message_content.mentions = Some(mentions.clone());

        let content = AnyMessageLikeEventContent::RoomMessage(message_content);
        timeline.send(content).await?;
    }

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn send_attachment(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    file: FileMetadata,
    room_id: String,
) -> Result<(), TauriError> {
    let client = matrix_client.read().await;
    let room = client
        .get_room(&RoomId::parse(&room_id)?)
        .ok_or("Room not found")?;

    let timeline = timeline_manager.get_or_create_timeline(&room).await?;

    let raw_bytes = match &file.source {
        UiAttachmentSource::LocalFile(path) => {
            let path = PathBuf::from(path);

            tokio::fs::read(&path).await?
        }
        UiAttachmentSource::RawBytes(bytes) => bytes.clone(),
    };

    let size = raw_bytes.len() as u32;

    let mime_type = Mime::from_str(&file.mime_type)?;
    let file_type = mime_type.type_();

    let info = match file_type {
        mime::IMAGE => {
            let dimensions = ImageReader::new(Cursor::new(raw_bytes)).with_guessed_format()?.into_dimensions().ok();

            let info = BaseImageInfo {
                width: dimensions.map(|(w, _)| w.into()),
                height: dimensions.map(|(_, h)| h.into()),
                size: Some(size.into()),
                blurhash: None,
                is_animated: None,
            };

            AttachmentInfo::Image(info)
        }
        mime::VIDEO => {
            let info = BaseVideoInfo {
                height: None,
                width: None,
                size: None,
                blurhash: None,
                duration: None,
            };

            AttachmentInfo::Video(info)
        }
        _ => {
            AttachmentInfo::File(BaseFileInfo {
                size: Some(size.into()),
            })
        }
    };

    let config = AttachmentConfig {
        txn_id: None,
        info: Some(info),
        thumbnail: None,
        caption: None,
        in_reply_to: None,
        mentions: None,
    };

    let source = match file.source {
        UiAttachmentSource::LocalFile(path) => {
             AttachmentSource::File(PathBuf::from(path))
        }
        UiAttachmentSource::RawBytes(bytes) => AttachmentSource::Data { bytes, filename: file.file_name }
    };

    timeline.send_attachment(source, mime_type, config).await?;

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn edit_message(
    html: String,
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    room_id: String,
    event_id: String,
) -> Result<(), TauriError> {
    log::debug!("Editing message {} in room {}", event_id, room_id);
    let client = matrix_client.read().await;
    let room = client
        .get_room(&RoomId::parse(&room_id)?)
        .ok_or("Room not found")?;

    let timeline = timeline_manager.get_or_create_timeline(&room).await?;

    let mut mentions = Mentions::default();
    let mut spans = Vec::new();

    let (body, formatted_body) = process_string_to_message(&html, &mut mentions, &mut spans);

    let mut messge_content =
        RoomMessageEventContentWithoutRelation::text_html(body, formatted_body);
    messge_content.mentions = Some(mentions);

    timeline
        .edit(
            &TimelineEventItemId::EventId(OwnedEventId::try_from(event_id)?),
            EditedContent::RoomMessage(messge_content),
        )
        .await?;

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
                    spans.push(RichTextSpan::RoomMention {
                        room_id: RoomIdFormat::Id(id.to_string()),
                        display_name: display_text.clone(),
                    });
                    body.push_str("@room");
                    mentions.room = true;
                    formatted.push_str(&format!(
                        "<a href=\"https://matrix.to/#/{}\">@{}</a>",
                        id, display_text
                    ));
                } else if data_type == "user_mention" {
                    if let Ok(user_id) = OwnedUserId::try_from(id) {
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
    media_manager: State<'_, MediaManager>,
    handle: AppHandle,
    room_id: String,
) -> Result<Vec<UiTimelineItem>, TauriError> {
    log::debug!("Fetching timeline for room {}", room_id);

    let token = CancellationToken::new();
    task_manager
        .replace_task("get_timeline", token.clone())
        .await;

    let mut media_store: HashMap<Uuid, MediaSource> = media_manager.sources.read().await.clone();

    tokio::select! {
        _ = token.cancelled() => {
            log::debug!("Timeline fetch for room {} was cancelled by a newer request", room_id);
            Ok(Vec::new())
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

            let media_for_stream = (*media_manager).clone();
            timeline_manager
                .set_stream_handle(tokio::spawn(async move {
                    tokio::pin!(stream);

                    while let Some(update) = stream.next().await {
                        let mut new_sources = HashMap::new();
                        let diffs = coalesce_diffs(update.iter().map(|v| {
                            let (res, ext) = timeline_diff_to_ui(v);
                            new_sources.extend(ext);
                            res
                        }).collect());
                        if !new_sources.is_empty() {
                            media_for_stream.sources.write().await.extend(new_sources);
                        }
                        send_timeline_diffs(handle.clone(), diffs);
                    }
                }))
                .await;

            log::debug!("Fetched {} messages for room {}", messages.len(), room_id);

            timeline.mark_as_read(ReceiptType::FullyRead).await?;

            let messages = messages.iter().map(|v| timeline_item_to_ui(v, &mut media_store)).collect();

            media_manager.sources.write().await.extend(media_store);

            Ok(messages)
        } => {
            result
        }
    }
}

fn send_timeline_diffs(handle: AppHandle, diffs: Vec<UiTimelineDiff>) {
    log::debug!("Emitting timeline update with {} diffs", diffs.len());
    if let Err(e) = handle.emit("timeline_update", diffs) {
        error!("Failed to emit timeline update: {:?}", e);
    }
}

#[command(rename_all = "snake_case")]
pub async fn toggle_reaction(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    room_id: String,
    event_id: String,
    reaction: String,
) -> Result<(), TauriError> {
    log::debug!(
        "Toggling reaction '{}' on event {} in room {}",
        reaction,
        event_id,
        room_id
    );
    let room = matrix_client
        .read()
        .await
        .get_room(&RoomId::parse(&room_id)?)
        .ok_or("No room found")?;

    let timeline = timeline_manager.get_or_create_timeline(&room).await?;

    timeline
        .toggle_reaction(
            &TimelineEventItemId::EventId(OwnedEventId::try_from(event_id)?),
            &reaction,
        )
        .await?;

    Ok(())
}
