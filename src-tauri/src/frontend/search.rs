use std::collections::HashSet;

use matrix_sdk::Client;
use matrix_sdk_ui::timeline::TimelineItemContent;
use shared::{
    api::{SearchParameters, events::SearchResultUpdate},
    timeline::{EventContent, UiTimelineItem},
};
use tauri::{AppHandle, State, async_runtime::spawn, command};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    TauriError,
    frontend::timeline::{load_reply_info, timeline_item_content_to_ui},
    send_event,
    state::TaskManager,
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

    for room_id in parameters.room_ids {
        let room = match client.get_room(&room_id) {
            Some(room) => room,
            None => {
                log::error!("Room with id {} not found", &room_id);
                continue;
            }
        };

        let mut stream = room.search_messages(query.clone(), SEARCH_BATCH_SIZE);

        let token_clone = token.clone();
        let handle_clone = handle.clone();
        let search_id_clone = search_id;
        spawn(async move {
            let mut sent_any = false;
            loop {
                tokio::select! {
                    _ = token_clone.cancelled() => {
                        log::trace!("Search task for room {room_id} cancelled");
                        return;
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
                            let ts: u64 = ts.as_secs().into();

                            let Some(event_id) = event.event_id() else {
                                continue;
                            };

                            let reply_info = load_reply_info(&room, event.raw()).await;

                            let Some(content) = TimelineItemContent::from_event(&room, event).await else {
                                continue;
                            };

                            let mut ui_content = timeline_item_content_to_ui(
                                &content,
                                None,
                                &mut HashSet::new(),
                            );

                            if let EventContent::MsgLike(msg) = &mut ui_content
                                && reply_info.is_some()
                            {
                                msg.in_reply_to = reply_info;
                            }

                            messages.push((
                                ts,
                                ui_content.to_timeline_item(
                                    event_id,
                                    sender,
                                    ts,
                                ),
                            ));
                        }

                        messages.sort_by_key(|msg| msg.0);
                        let messages: Vec<UiTimelineItem> =
                            messages.into_iter().rev().map(|(_, msg)| msg).collect();

                        let payload: SearchResultUpdate = (search_id_clone, room_id.clone(), messages);

                        send_event(&handle_clone, &payload);
                        sent_any = true;
                    }
                }
            }

            // The frontend keeps showing the previous search's results for
            // this room until an update arrives, so always send one.
            if !sent_any {
                let payload: SearchResultUpdate = (search_id_clone, room_id, Vec::new());
                send_event(&handle_clone, &payload);
            }
        });
    }

    Ok(())
}
