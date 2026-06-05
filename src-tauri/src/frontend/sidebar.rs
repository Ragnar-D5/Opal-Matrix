use async_recursion::async_recursion;
use matrix_sdk::ruma::MilliSecondsSinceUnixEpoch;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use futures::{StreamExt, stream};
use matrix_sdk::{Client, Room};

use matrix_sdk::room::ParentSpace;
use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
use matrix_sdk::sync::RoomUpdates;
use matrix_sdk::ruma::events::call::member::CallMemberEventContent;
use matrix_sdk::ruma::events::{AnyStateEventContent, AnySyncStateEvent, StateEventType};
use shared::sidebar::{NotificationCounts, RoomKind, RoomNode, SidebarState, VoiceParticipants};
use tauri::AppHandle;
use tauri::Emitter;

use crate::TauriError;

pub async fn send_sidebar(all_rooms: &[Room], handle: &AppHandle, own_id: &str) -> Result<(), TauriError> {
    let mut channels = HashMap::new();
    let mut dms: Vec<Room> = stream::iter(all_rooms.iter().cloned()).filter_map(|room| async move {
        let res = room.compute_is_dm().await;
        if res.unwrap_or(false) {
            Some(room)
        } else {
            None
        }
    }).collect().await;

    dms.sort_by(|a, b| {
        let a_ts = a.latest_event_timestamp().unwrap_or(MilliSecondsSinceUnixEpoch(0u32.into()));
        let b_ts = b.latest_event_timestamp().unwrap_or(MilliSecondsSinceUnixEpoch(0u32.into()));
        b_ts.cmp(&a_ts)
    });

    let dms = stream::iter(dms).filter_map(|room| async move {
        let other_user_ids = room.joined_user_ids().await.ok()?;
        let other_user_id = other_user_ids.into_iter().find(|id| id != own_id)?;

        Some(RoomNode {
            room_id: room.room_id().to_string(),
            name: room.display_name().await.ok().map(|n| n.to_string()),
            topic: room.topic(),
            kind: RoomKind::Dm { other_user_id: other_user_id.to_string() },
        })
    }).collect().await;

    for room in all_rooms {
        channels.insert(room.room_id().to_string(), room);
    }

    let mut children_to_parents: HashMap<&String, Vec<Room>> = HashMap::new();
    for (id, room) in &channels {
        let stream = room.parent_spaces().await?;
        let results: Vec<Result<ParentSpace, _>> = stream.collect().await;

        let result: Result<Vec<ParentSpace>, _> = results.into_iter().collect();

        let mut actual_parents = Vec::new();
        for parent in result.unwrap_or_default() {
            if let ParentSpace::Reciprocal(room) = parent {
                actual_parents.push(room);
            }
        }

        children_to_parents.insert(id, actual_parents);
    }

    let mut parent_to_children: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_children: HashSet<String> = HashSet::new();

    for (child_id, parents) in &children_to_parents {
        for parent in parents {
            let parent_id = parent.room_id().to_string();

            parent_to_children
                .entry(parent_id)
                .or_default()
                .push((*child_id).clone());

            all_children.insert((*child_id).clone());
        }
    }

    let mut ordered_parent_to_children: HashMap<String, Vec<(String, Option<String>)>> =
        HashMap::new();

    for (parent_id, child_ids) in &parent_to_children {
        if let Some(parent_room) = channels.get(parent_id) {
            let child_events = parent_room
                .get_state_events_static::<SpaceChildEventContent>()
                .await
                .unwrap_or_default();

            let order_map: HashMap<String, Option<String>> = child_events
                .iter()
                .filter_map(|raw| raw.deserialize().ok())
                .filter_map(|ev| {
                    ev.as_stripped().map(|or| {
                        (
                            ev.state_key().to_string(),
                            or.content.order.clone().clone().map(|o| o.to_string()),
                        )
                    })
                })
                .collect();

            let mut children_with_order: Vec<(String, Option<String>)> = child_ids
                .iter()
                .map(|id| (id.clone(), order_map.get(id).cloned().flatten()))
                .collect();

            children_with_order.sort_by(|(id_a, ord_a), (id_b, ord_b)| match (ord_a, ord_b) {
                (Some(a), Some(b)) => a.cmp(b),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => id_a.cmp(id_b),
            });

            ordered_parent_to_children.insert(parent_id.clone(), children_with_order);
        }
    }

    // 2. Build the top-level servers (Spaces with no parents)
    let mut top_level_servers = Vec::new();
    for room_id in channels.keys() {
        let is_space = if let Some(room) = channels.get(room_id) {
            room.is_space()
        } else {
            false
        };

        // A server is a space that is not a child of any other space
        if is_space
            && !all_children.contains(room_id)
            && let Some(node) =
                build_async_node(room_id, &channels, &ordered_parent_to_children).await
        {
            top_level_servers.push(node);
        }
    }

    // 3. Find orphaned channels (not a DM, not a space, and has no parent space)
    let mut orphaned_channels = Vec::new();
    for (room_id, room) in &channels {
        if !room.is_space() && !all_children.contains(room_id) {
            orphaned_channels.push(RoomNode {
                room_id: room_id.clone(),
                name: room.display_name().await.ok().map(|n| n.to_string()),
                topic: room.topic(),
                kind: RoomKind::TextChannel,
            });
        }
    }

    let sidebar_state = SidebarState {
        dms,
        servers: top_level_servers,
        orphaned_rooms: orphaned_channels,
    };

    handle.emit("sidebar_update", sidebar_state)?;

    Ok(())
}


#[async_recursion]
async fn build_async_node(
    room_id: &str,
    channels: &HashMap<String, &Room>,
    parent_to_children: &HashMap<String, Vec<(String, Option<String>)>>,
) -> Option<RoomNode> {
    let room = channels.get(room_id)?;

    let mut user_ids_in_calls = HashSet::new();

    let room_kind = if room.is_space() {
        let mut children_nodes = Vec::new();

        if let Some(child_ids) = parent_to_children.get(room_id) {
            for (child_id, _) in child_ids {
                if let Some(child_node) =
                    build_async_node(child_id, channels, parent_to_children).await
                {
                    if let RoomKind::VoiceChannel { participants } = &child_node.kind {
                        for user_id in participants.keys() {
                            user_ids_in_calls.insert(user_id.clone());
                        }
                    }
                    children_nodes.push(child_node);
                }
            }
        }

        RoomKind::Space {
            user_ids_in_calls: user_ids_in_calls.into_iter().collect(),
            children: children_nodes,
        }
    } else {
        if room.is_call() {
            let mut participants: VoiceParticipants = VoiceParticipants::new();
            let state_events = room
                .get_state_events(StateEventType::CallMember)
                .await
                .unwrap_or_default();
            for raw_event in state_events {
                let Ok(event) = raw_event.deserialize() else { continue };
                let Some(event) = event.as_sync() else { continue };
                let user_id = event.sender().to_string();
                let Some(AnyStateEventContent::CallMember(
                    CallMemberEventContent::SessionContent(content),
                )) = event.original_content()
                else {
                    continue;
                };
                participants
                    .entry(user_id)
                    .or_default()
                    .push(content.device_id.to_string());
            }
            RoomKind::VoiceChannel { participants }
        } else {
            RoomKind::TextChannel
        }
    };

    Some(RoomNode {
        room_id: room_id.to_string(),
        name: room.display_name().await.ok().map(|n| n.to_string()),
        topic: room.topic(),
        kind: room_kind,
    })
}

fn is_structural_state_event(event: &AnySyncStateEvent, own_user_id: &str) -> bool {
    match event {
        AnySyncStateEvent::RoomMember(ev) => ev.state_key().as_str() == own_user_id,
        AnySyncStateEvent::RoomName(_)
        | AnySyncStateEvent::RoomTopic(_)
        | AnySyncStateEvent::RoomAvatar(_)
        | AnySyncStateEvent::SpaceChild(_) => true,
        _ => false,
    }
}

fn should_sidebar_update(room_updates: &RoomUpdates, own_user_id: &str) -> bool {
    use matrix_sdk::sync::State;
    use matrix_sdk::ruma::events::AnySyncTimelineEvent;

    if !room_updates.left.is_empty() || !room_updates.invited.is_empty() {
        return true;
    }

    for update in room_updates.joined.values() {
        let state_block = match &update.state {
            State::Before(events) | State::After(events) => events,
        };

        for raw in state_block {
            if let Ok(event) = raw.deserialize()
                && is_structural_state_event(&event, own_user_id)
            {
                return true;
            }
        }

        for timeline_event in &update.timeline.events {
            if let Ok(AnySyncTimelineEvent::State(event)) = timeline_event.raw().deserialize()
                && is_structural_state_event(&event, own_user_id)
            {
                return true;
            }
        }

        for raw in &update.account_data {
            if let Ok(Some(event_type)) = raw.get_field::<String>("type")
                && event_type == "m.direct"
            {
                return true;
            }
        }
    }

    false
}

fn should_notification_update(room_updates: &RoomUpdates) -> bool {
    for update in room_updates.joined.values() {
        if !update.timeline.events.is_empty() {
            return true;
        }
        for raw_event in &update.ephemeral {
            if let Ok(Some(event_type)) = raw_event.get_field::<String>("type")
                && event_type == "m.receipt"
            {
                return true;
            }
        }
        for raw_event in &update.account_data {
            if let Ok(Some(event_type)) = raw_event.get_field::<String>("type")
                && event_type == "m.fully_read"
            {
                return true;
            }
        }
    }
    false
}

pub async fn handle_room_updates(room_updates: &RoomUpdates, client: &Client, handle: &AppHandle, own_id: &str) {
    if should_sidebar_update(room_updates, own_id) {
        log::debug!("Refreshing sidebar");
        if let Err(e) = send_sidebar(&client.joined_rooms(), handle, own_id).await {
            log::error!("Failed to send sidebar update: {:?}", e);
        }
    }

    if should_notification_update(room_updates) {
        let counts = get_notification_counts(client).await;
        if let Err(e) = handle.emit("notification_counts_update", counts) {
            log::error!("Failed to emit notification counts: {:?}", e);
        }
    }
}

pub async fn get_notification_counts(client: &Client) -> HashMap<String, NotificationCounts> {
    client.rooms().iter().map(|room| {
        let notification_count = room.num_unread_notifications();
        let highlight_count = room.num_unread_mentions();

        (room.room_id().to_string(), NotificationCounts {
            notification_count,
            highlight_count,
        })
    }).collect()
}
