use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
    path::PathBuf,
    str::FromStr,
};

use futures::StreamExt;
use image::ImageReader;
use matrix_sdk::{
    Client as MatrixClient,
    attachment::{AttachmentInfo, BaseFileInfo, BaseImageInfo, BaseVideoInfo},
    room::edit::EditedContent,
    ruma::{OwnedRoomId, events::room::MediaSource},
};
use matrix_sdk_ui::timeline::{
    AttachmentConfig, AttachmentSource, TimelineEventItemId, TimelineItemContent,
};
use mime::Mime;
use tauri_plugin_http::reqwest;
use tokio_util::sync::CancellationToken;

use ego_tree::NodeRef;
use log::warn;
use matrix_sdk::ruma::{
    OwnedEventId, OwnedUserId,
    events::{
        AnyMessageLikeEventContent, Mentions,
        room::message::{RoomMessageEventContent, RoomMessageEventContentWithoutRelation},
    },
};
use scraper::{Html, Node};
use shared::{
    api::{FileMetadata, GetTimelineResult, ScrollDirection, UiAttachmentSource},
    timeline::{EventContent, UiTimelineDiff, UiTimelineItem, coalesce_diffs},
};
use tauri::{State, command, ipc::Channel};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    TauriError,
    frontend::timeline::{
        load_reply_info, timeline_diff_to_ui, timeline_item_content_to_ui, timeline_item_to_ui,
    },
    state::{MediaManager, TaskManager, TimelineManager},
};

#[command(rename_all = "snake_case")]
pub async fn commit_message(
    html: String,
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    room_id: OwnedRoomId,
    replies_to: Option<OwnedEventId>,
) -> Result<(), TauriError> {
    log::debug!("Committing message to room {}", room_id);
    let client = matrix_client.read().await;
    let room = client.get_room(&room_id).ok_or("Room not found")?;

    let (_, timeline) = timeline_manager.get_or_create_timeline(&room, None).await?;

    let mut mentions = Mentions::default();

    let (body, formatted_body) = process_string_to_message(&html, &mut mentions);

    if let Some(reply_to_id) = replies_to {
        let content = if let Some(formatted_body) = formatted_body {
            RoomMessageEventContentWithoutRelation::text_html(body, formatted_body)
        } else {
            RoomMessageEventContentWithoutRelation::text_plain(body)
        };
        timeline.send_reply(content, reply_to_id).await?;
    } else {
        let mut message_content = if let Some(formatted_body) = formatted_body {
            RoomMessageEventContent::text_html(body, formatted_body)
        } else {
            RoomMessageEventContent::text_plain(body)
        };
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
    room_id: OwnedRoomId,
) -> Result<(), TauriError> {
    let client = matrix_client.read().await;
    let room = client.get_room(&room_id).ok_or("Room not found")?;

    let (_, timeline) = timeline_manager.get_or_create_timeline(&room, None).await?;

    let raw_bytes = match &file.source {
        UiAttachmentSource::LocalFile(path) => {
            let path = PathBuf::from(path);

            tokio::fs::read(&path).await?
        }
        UiAttachmentSource::RawBytes(bytes) => bytes.clone(),
        UiAttachmentSource::Url(url) => {
            let response = reqwest::get(url).await?;
            response.bytes().await?.to_vec()
        }
    };

    let size = raw_bytes.len() as u32;

    let mime_type = Mime::from_str(&file.mime_type)?;
    let file_type = mime_type.type_();

    let info = match file_type {
        mime::IMAGE => {
            let img = ImageReader::new(Cursor::new(&raw_bytes))
                .with_guessed_format()
                .ok()
                .and_then(|r| r.decode().ok());

            let dimensions = img.as_ref().map(|i| (i.width(), i.height()));

            let bh = img.as_ref().and_then(|img| {
                let thumb = img.thumbnail(64, 64);
                let rgba = thumb.to_rgba8();
                blurhash::encode(4, 3, rgba.width(), rgba.height(), &rgba).ok()
            });

            let info = BaseImageInfo {
                width: dimensions.map(|(w, _)| w.into()),
                height: dimensions.map(|(_, h)| h.into()),
                size: Some(size.into()),
                blurhash: bh,
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
        _ => AttachmentInfo::File(BaseFileInfo {
            size: Some(size.into()),
        }),
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
        UiAttachmentSource::LocalFile(path) => AttachmentSource::File(PathBuf::from(path)),
        UiAttachmentSource::RawBytes(bytes) => AttachmentSource::Data {
            bytes,
            filename: file.file_name,
        },
        UiAttachmentSource::Url(_) => AttachmentSource::Data {
            bytes: raw_bytes,
            filename: file.file_name,
        },
    };

    timeline.send_attachment(source, mime_type, config).await?;

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn edit_message(
    html: String,
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    room_id: OwnedRoomId,
    event_id: OwnedEventId,
) -> Result<(), TauriError> {
    log::debug!("Editing message {} in room {}", event_id, room_id);
    let client = matrix_client.read().await;
    let room = client.get_room(&room_id).ok_or("Room not found")?;

    let (_, timeline) = timeline_manager
        .get_or_create_timeline(&room, Some(event_id.clone()))
        .await?;

    let mut mentions = Mentions::default();

    let (body, formatted_body) = process_string_to_message(&html, &mut mentions);

    let mut messge_content = if let Some(formatted_body) = formatted_body {
        RoomMessageEventContentWithoutRelation::text_html(body, formatted_body)
    } else {
        RoomMessageEventContentWithoutRelation::text_plain(body)
    };
    messge_content.mentions = Some(mentions);

    timeline
        .edit(
            &TimelineEventItemId::EventId(event_id),
            EditedContent::RoomMessage(messge_content),
        )
        .await?;

    Ok(())
}

fn process_string_to_message(html: &str, mentions: &mut Mentions) -> (String, Option<String>) {
    let fragment = Html::parse_fragment(html);

    let mut body = String::new();
    let mut formatted_body = String::new();

    for node in fragment.tree.root().children() {
        walk_node(node, mentions, &mut body, &mut formatted_body);
    }

    let formatted_body = (formatted_body != body).then_some(formatted_body);

    (body, formatted_body)
}

fn walk_node(
    node: NodeRef<'_, Node>,
    mentions: &mut Mentions,
    body: &mut String,
    formatted: &mut String,
) {
    match node.value() {
        Node::Text(text) => {
            let content = text.text.replace("\u{a0}", " ");
            body.push_str(&content);
            formatted.push_str(&content);
        }
        Node::Element(elem) => {
            if let Some(url) = elem.attr("data-url") {
                let display_text = extract_text(node);
                body.push_str(&display_text);
                formatted.push_str(&format!("<a href=\"{}\">{}</a>", url, display_text));
                return;
            }
            if let Some(data_type) = elem.attr("data-type")
                && let Some(id) = elem.attr("data-id")
            {
                let display_text = extract_text(node).trim_start_matches('@').to_string();

                if data_type == "room_mention" {
                    body.push_str("#room");
                    mentions.room = true;
                    formatted.push_str(&format!(
                        "<a href=\"https://matrix.to/#/{}\">{}</a>",
                        id, display_text
                    ));
                } else if data_type == "user_mention" {
                    if let Ok(user_id) = OwnedUserId::try_from(id) {
                        mentions.user_ids.insert(user_id);
                    } else {
                        warn!("Invalid user ID in mention: {id}");
                    }

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
                        walk_node(child, mentions, body, formatted);
                    }
                }
                "br" => {
                    body.push('\n');
                    formatted.push_str("<br>");
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
pub async fn scroll_timeline(
    timeline_manager: State<'_, TimelineManager>,
    timeline_id: String,
    direction: ScrollDirection,
) -> Result<bool, TauriError> {
    let id = Uuid::parse_str(&timeline_id).map_err(|e| format!("Invalid timeline ID: {e}"))?;
    let timeline = timeline_manager
        .get_timeline_by_id(id)
        .await
        .ok_or("Timeline not found")?;

    let hit_end = match direction {
        ScrollDirection::Up => timeline.paginate_backwards(30).await?,
        ScrollDirection::Down => timeline.paginate_forwards(30).await?,
    };

    log::debug!("Pagination result: hit_end={hit_end}");

    Ok(!hit_end)
}

#[allow(clippy::too_many_arguments)]
#[command(rename_all = "snake_case")]
pub async fn get_timeline(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    task_manager: State<'_, TaskManager>,
    media_manager: State<'_, MediaManager>,
    room_id: OwnedRoomId,
    event_id: Option<OwnedEventId>,
    channel: Channel<Vec<UiTimelineDiff>>,
) -> Result<GetTimelineResult, TauriError> {
    log::debug!(
        "Fetching timeline for room {}{}",
        room_id,
        event_id
            .as_ref()
            .map(|id| format!(" at event {}", id))
            .unwrap_or_default()
    );

    let token = CancellationToken::new();
    task_manager
        .replace_task("get_timeline", token.clone())
        .await;

    let mut media_store: HashMap<Uuid, MediaSource> = media_manager.sources.read().await.clone();

    let room = matrix_client
        .read()
        .await
        .get_room(&room_id)
        .ok_or("No room found")?;

    tokio::select! {
        _ = token.cancelled() => {
            log::debug!("Timeline fetch for room {} was cancelled by a newer request", room_id);
            Ok(GetTimelineResult { timeline_id: Uuid::nil().to_string(), messages: Vec::new() })
        }

        result = async {
            timeline_manager.abort_stream().await;

            let (timeline_id, timeline) = timeline_manager.get_or_create_timeline(&room, event_id).await?;

            let (messages, stream) = timeline.subscribe().await;

            let media_for_stream = (*media_manager).clone();
            let timeline_for_stream = timeline.clone();
            timeline_manager
                .set_stream_handle(tokio::spawn(async move {
                    tokio::pin!(stream);

                    while let Some(update) = stream.next().await {
                        let mut new_sources = HashMap::new();
                        let mut unknown_reply_event_ids = HashSet::new();

                        let diffs = coalesce_diffs(update.iter().map(|v| timeline_diff_to_ui(v, &mut new_sources, &mut unknown_reply_event_ids)).collect());
                        if !new_sources.is_empty() {
                            media_for_stream.sources.write().await.extend(new_sources);
                        }

                        log::trace!("Sending timeline update");
                        if let Err(e) = channel.send(diffs.clone()) {
                            warn!("Failed to send timeline update: {:?}", e);
                        }

                        for event_id in unknown_reply_event_ids {
                            log::debug!("Fetching unknown reply event {}", event_id);
                            if let Err(e) = timeline_for_stream.fetch_details_for_event(&event_id).await {
                                warn!("Failed to fetch details for event {}: {:?}", event_id, e);
                            }
                        }
                    }
                }))
                .await;

            log::debug!("Fetched {} messages for room {}", messages.len(), room_id);

            let mut unknown_reply_event_ids = HashSet::new();
            let ui_messages: Vec<_> = messages.iter().map(|v| timeline_item_to_ui(v, &mut media_store, &mut unknown_reply_event_ids)).collect();

            if !unknown_reply_event_ids.is_empty() {
                let timeline_bg = timeline.clone();
                tokio::spawn(async move {
                    for event_id in unknown_reply_event_ids {
                        log::debug!("Fetching unknown reply event {}", event_id);
                        if let Err(e) = timeline_bg.fetch_details_for_event(&event_id).await {
                            warn!("Failed to fetch details for event {}: {:?}", event_id, e);
                        }
                    }
                });
            }

            media_manager.sources.write().await.extend(media_store);

            Ok(GetTimelineResult { timeline_id: timeline_id.to_string(), messages: ui_messages })
        } => {
            result
        }
    }
}

#[command(rename_all = "snake_case")]
pub async fn toggle_reaction(
    client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    room_id: OwnedRoomId,
    event_id: OwnedEventId,
    reaction: String,
) -> Result<(), TauriError> {
    log::debug!(
        "Toggling reaction '{}' on event {} in room {}",
        reaction,
        event_id,
        room_id
    );
    let client = client.read().await;

    let room = client.get_room(&room_id).ok_or("No room found")?;

    let (_, timeline) = timeline_manager
        .get_or_create_timeline(&room, Some(event_id.clone()))
        .await?;

    let own_id = client.user_id().ok_or("No user ID found")?.to_owned();

    let mut is_adding = true;
    if let Some(item) = timeline.item_by_event_id(&event_id).await
        && let Some(reactions) = item.content().reactions()
        && reactions
            .get(&reaction)
            .map(|r| r.contains_key(&own_id))
            .unwrap_or(false)
    {
        is_adding = false;
    }

    timeline
        .toggle_reaction(&TimelineEventItemId::EventId(event_id), &reaction)
        .await?;

    if is_adding {
        client.account().add_recent_emoji(&reaction).await?;
    }

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn delete_message(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    timeline_manager: State<'_, TimelineManager>,
    room_id: OwnedRoomId,
    event_id: OwnedEventId,
) -> Result<(), TauriError> {
    log::debug!("Deleting message {} in room {}", event_id, room_id);
    let room = matrix_client
        .read()
        .await
        .get_room(&room_id)
        .ok_or("No room found")?;

    let (_, timeline) = timeline_manager
        .get_or_create_timeline(&room, Some(event_id.clone()))
        .await?;

    timeline
        .redact(&TimelineEventItemId::EventId(event_id), None)
        .await?;

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn indicate_typing(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    room_id: OwnedRoomId,
    is_typing: bool,
) -> Result<(), TauriError> {
    let client = matrix_client.read().await;
    let room = client.get_room(&room_id).ok_or("Room not found")?;

    room.typing_notice(is_typing).await?;

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn get_pinned_events(
    client: State<'_, RwLock<MatrixClient>>,
    media_manager: State<'_, MediaManager>,
    room_id: OwnedRoomId,
) -> Result<Vec<UiTimelineItem>, TauriError> {
    let client = client.read().await;
    let room = client.get_room(&room_id).ok_or("Room not found")?;

    let Some(pinned_event_ids) = room.pinned_event_ids() else {
        return Ok(Vec::new());
    };

    let mut media_store = HashMap::new();

    let mut messages = Vec::new();
    for event_id in pinned_event_ids {
        let event = match room.load_or_fetch_event(&event_id, None).await {
            Ok(ev) => ev,
            Err(e) => {
                log::warn!("Failed to fetch pinned event {}: {:?}", event_id, e);
                continue;
            }
        };

        let Some(sender) = event.sender() else {
            continue;
        };
        let Some(ts) = event.timestamp else {
            continue;
        };
        let ts: u64 = ts.as_secs().into();

        let reply_info = load_reply_info(&room, event.raw()).await;

        let Some(content) = TimelineItemContent::from_event(&room, event).await else {
            continue;
        };

        let mut ui_content =
            timeline_item_content_to_ui(&content, &mut media_store, None, &mut HashSet::new());

        if let EventContent::MsgLike(msg) = &mut ui_content
            && reply_info.is_some()
        {
            msg.in_reply_to = reply_info;
        }

        messages.push((ts, ui_content.to_timeline_item(event_id, sender, ts)));
    }

    media_manager.sources.write().await.extend(media_store);

    messages.sort_by_key(|(ts, _)| *ts);
    let messages = messages.into_iter().map(|(_, msg)| msg).collect();

    Ok(messages)
}

#[command(rename_all = "snake_case")]
pub async fn pin_event(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    room_id: OwnedRoomId,
    event_id: OwnedEventId,
) -> Result<(), TauriError> {
    let client = matrix_client.read().await;
    let room = client.get_room(&room_id).ok_or("Room not found")?;

    room.pin_event(&event_id).await?;

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn unpin_event(
    matrix_client: State<'_, RwLock<MatrixClient>>,
    room_id: OwnedRoomId,
    event_id: OwnedEventId,
) -> Result<(), TauriError> {
    let client = matrix_client.read().await;
    let room = client.get_room(&room_id).ok_or("Room not found")?;

    room.unpin_event(&event_id).await?;

    Ok(())
}
