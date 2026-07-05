use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use chrono::{TimeZone, Utc};
use matrix_sdk::{
    ruma::{OwnedRoomId, UInt},
    Client,
};
use matrix_sdk_ui::timeline::TimelineItemContent;
use shared::{
    api::RoomSearchParameters,
    timeline::{UiTimelineItem, UiTimelineItemKind},
};
use tauri::{command, State};
use tokio::sync::RwLock;

use crate::{frontend::timeline::timeline_item_content_to_ui, TauriError};

const SEARCH_LIMIT: usize = 20;
const SEARCH_CONTEXT_SIZE: u64 = 0;

#[command(rename_all = "snake_case")]
pub async fn search_room(
    client: State<'_, RwLock<Client>>,
    parameters: RoomSearchParameters,
    offset: usize,
) -> Result<HashMap<String, Vec<UiTimelineItem>>, TauriError> {
    let offset = if offset == 0 {
        None
    } else {
        Some(offset * SEARCH_LIMIT)
    };

    let client = client.read().await;

    let mut results = Vec::new();

    let query = parameters.build_query();

    for room_id in &parameters.room_ids {
        let room_id = match OwnedRoomId::from_str(room_id) {
            Ok(id) => id,
            Err(e) => {
                log::error!("Invalid room ID: {room_id}, error: {e}");
                continue;
            }
        };
        let room = match client.get_room(&room_id) {
            Some(room) => room,
            None => {
                log::error!("Room with id {room_id} not found");
                continue;
            }
        };

        let ids = room.search(&query, SEARCH_LIMIT, offset).await?;

        results.push((room, ids));
    }

    let mut seen_ids = HashSet::new();

    let mut message_map = HashMap::new();

    for (room, ids) in results {
        let mut messages = Vec::new();

        for event_id in ids {
            let context = room
                .event_with_context(
                    &event_id,
                    true,
                    UInt::new_saturating(SEARCH_CONTEXT_SIZE),
                    None,
                )
                .await?;

            let Some(event) = context.event else {
                continue;
            };
            let Some(sender) = event.sender() else {
                continue;
            };
            let Some(ts) = event.timestamp else {
                continue;
            };
            let Some(content) = TimelineItemContent::from_event(&room, event).await else {
                continue;
            };
            let ts: u64 = ts.as_secs().into();

            if !seen_ids.insert(event_id.clone()) {
                continue;
            }

            messages.push((
                ts,
                timeline_item_content_to_ui(
                    &content,
                    &mut HashMap::new(),
                    None,
                    &mut HashSet::new(),
                )
                .to_timeline_item(event_id.to_string(), sender.to_string(), ts),
            ));
        }
        messages.sort_by_key(|msg| msg.0);

        let mut final_messages = Vec::with_capacity(messages.len());
        let mut current_day: Option<String> = None;

        for (ts, msg) in messages {
            let msg_date = match Utc.timestamp_millis_opt(ts as i64 * 1000) {
                chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%d").to_string(),
                _ => continue,
            };

            if current_day.as_ref() != Some(&msg_date) {
                current_day = Some(msg_date.clone());
                final_messages.push(UiTimelineItem {
                    id: format!("date_separator_{}", msg_date),
                    kind: UiTimelineItemKind::DateDivider(ts),
                });
            }

            final_messages.push(msg);
        }

        message_map.insert(room.room_id().to_string(), final_messages);
    }

    Ok(message_map)
}
