use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use matrix_sdk::{ruma::OwnedRoomId, Client};
use matrix_sdk_ui::timeline::TimelineItemContent;
use shared::{
    api::{events::SearchResultUpdate, SearchParameters},
    timeline::UiTimelineItem,
};
use tauri::{async_runtime::spawn, command, AppHandle, State};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    frontend::timeline::timeline_item_content_to_ui, send_event, state::TaskManager, TauriError,
};

const SEARCH_BATCH_SIZE: usize = 20;

#[command(rename_all = "snake_case")]
pub async fn search_rooms(
    client: State<'_, RwLock<Client>>,
    task_manager: State<'_, TaskManager>,
    parameters: SearchParameters,
    search_id: Uuid,
    handle: AppHandle,
) -> Result<(), TauriError> {
    let token = CancellationToken::new();
    task_manager
        .replace_task("search_room", token.clone())
        .await;

    let client = client.read().await;

    let query = parameters.build_query();
    log::trace!(
        "Searching for query: {query} in rooms: {:?}",
        parameters.room_ids
    );

    for room_id_str in &parameters.room_ids {
        let room_id = match OwnedRoomId::from_str(room_id_str) {
            Ok(id) => id,
            Err(e) => {
                log::error!("Invalid room ID: {room_id_str}, error: {e}");
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

        let mut stream = room.search_messages(query.clone(), SEARCH_BATCH_SIZE);

        let token_clone = token.clone();
        let handle_clone = handle.clone();
        let search_id_clone = search_id;
        spawn(async move {
            loop {
                tokio::select! {
                    _ = token_clone.cancelled() => {
                        log::trace!("Search task for room {room_id} cancelled");
                        break;
                    }
                    next = stream.next_events() => {
                        let next = match next {
                            Ok(events) => events,
                            Err(e) => {
                                log::error!("Error fetching search results: {e}");
                                break;
                            }
                        };
                        let Some(events) = next else {
                            log::trace!("No more search results for room {room_id}");
                            break;
                        };

                        let mut messages = Vec::new();
                        for event in events {
                            let Some(sender) = event.sender() else {
                                continue;
                            };
                            let Some(ts) = event.timestamp else {
                                continue;
                            };
                            let Some(event_id) = event.event_id() else {
                                continue;
                            };

                            let Some(content) = TimelineItemContent::from_event(&room, event).await else {
                                continue;
                            };
                            let ts: u64 = ts.as_secs().into();

                            messages.push((
                                ts,
                                timeline_item_content_to_ui(
                                    &content,
                                    &mut HashMap::new(),
                                    None,
                                    &mut HashSet::new(),
                                )
                                .to_timeline_item(
                                    event_id.to_string(),
                                    sender.to_string(),
                                    ts,
                                ),
                            ));
                        }

                        messages.sort_by_key(|msg| msg.0);
                        let messages: Vec<UiTimelineItem> =
                            messages.into_iter().map(|(_, msg)| msg).collect();

                        let payload: SearchResultUpdate = (search_id_clone, room_id.to_string(), messages);

                        send_event(&handle_clone, &payload);
                    }
                }
            }
        });
    }

    Ok(())
}
