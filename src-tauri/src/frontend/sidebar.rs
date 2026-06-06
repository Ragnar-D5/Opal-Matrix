use async_recursion::async_recursion;
use matrix_sdk::deserialized_responses::SyncOrStrippedState;
use matrix_sdk::ruma::MilliSecondsSinceUnixEpoch;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use futures::{StreamExt, stream};
use matrix_sdk::{Client, Room};

use matrix_sdk::room::ParentSpace;
use matrix_sdk::ruma::events::call::member::CallMemberEventContent;
use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
use matrix_sdk::ruma::events::{AnySyncStateEvent, AnySyncTimelineEvent, OriginalSyncStateEvent};
use matrix_sdk::sync::RoomUpdates;
use shared::sidebar::{NotificationCounts, RoomKind, RoomNode, SidebarState, UserDevice};
use tauri::AppHandle;
use tauri::Emitter;

use crate::TauriError;

async fn node_from_room(room: &Room, kind: RoomKind) -> RoomNode {
    let info = room.clone_info();
    let name = room
        .display_name()
        .await
        .ok()
        .map(|n| n.to_string());

    RoomNode {
        room_id: info.room_id().to_string(),
        name,
        topic: room.topic(),
        has_avatar: room.avatar_url().is_some(),
        canonical_alias: room.canonical_alias().map(|v| v.to_string()),
        aliases: info.alt_aliases().iter().map(|v| v.to_string()).collect(),
        kind,
    }
}

pub async fn send_sidebar(
    all_rooms: &[Room],
    handle: &AppHandle,
    own_id: &str,
) -> Result<(), TauriError> {
    let mut channels = HashMap::new();
    let mut dms: Vec<Room> = stream::iter(all_rooms.iter().cloned())
        .filter_map(|room| async move {
            let res = room.compute_is_dm().await;
            if res.unwrap_or(false) {
                Some(room)
            } else {
                None
            }
        })
        .collect()
        .await;

    dms.sort_by(|a, b| {
        let a_ts = a
            .latest_event_timestamp()
            .unwrap_or(MilliSecondsSinceUnixEpoch(0u32.into()));
        let b_ts = b
            .latest_event_timestamp()
            .unwrap_or(MilliSecondsSinceUnixEpoch(0u32.into()));
        b_ts.cmp(&a_ts)
    });

    let dms = stream::iter(dms)
        .filter_map(|room| async move {
            let other_user_ids = room.joined_user_ids().await.ok()?;
            let other_user_id = other_user_ids.into_iter().find(|id| id != own_id)?;

            Some(node_from_room(&room, RoomKind::Dm {
                other_user_id: other_user_id.to_string(),
            }).await)
        })
        .collect()
        .await;

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

    let mut server_rooms: HashMap<String, RoomNode> = HashMap::new();
    let mut top_level_servers = Vec::new();

    for room_id in channels.keys() {
        if channels[room_id].is_space() && !all_children.contains(room_id)
            && let Some(node) =
                build_async_node(room_id, &channels, &ordered_parent_to_children, &mut server_rooms).await
            {
                top_level_servers.push(node.room_id.clone());
                server_rooms.insert(node.room_id.clone(), node);
            }
    }

    let mut orphaned_rooms = Vec::new();
    for (room_id, room) in &channels {
        if !server_rooms.contains_key(room_id) && !room.is_space() && !all_children.contains(room_id) {
            orphaned_rooms.push(node_from_room(room, RoomKind::TextChannel).await);
        }
    }

    let sidebar_state = SidebarState {
        dms,
        top_level_servers,
        orphaned_rooms,
        server_rooms,
    };

    log::debug!("Emitting sidebar update with {} top-level servers, {} orphaned rooms and {} DMs",
        sidebar_state.top_level_servers.len(),
        sidebar_state.orphaned_rooms.len(),
        sidebar_state.dms.len(),
    );
    handle.emit("sidebar_update", sidebar_state)?;

    Ok(())
}

#[async_recursion]
async fn build_async_node(
    room_id: &str,
    channels: &HashMap<String, &Room>,
    parent_to_children: &HashMap<String, Vec<(String, Option<String>)>>,
    server_rooms: &mut HashMap<String, RoomNode>,
) -> Option<RoomNode> {
    let room = channels.get(room_id)?;

    let room_kind = if room.is_space() {
        let mut children_ids = Vec::new();
        let mut all_children_ids = HashSet::new();

        if let Some(child_ids) = parent_to_children.get(room_id) {
            for (child_id, _) in child_ids {
                if let Some(child_node) =
                    build_async_node(child_id, channels, parent_to_children, server_rooms).await
                {
                    if let RoomKind::Space { all_children, .. } = &child_node.kind {
                        all_children_ids.extend(all_children.clone());
                    }
                    all_children_ids.insert(child_node.room_id.clone());
                    children_ids.push(child_node.room_id.clone());
                    server_rooms.insert(child_node.room_id.clone(), child_node);
                }
            }
        }

        RoomKind::Space {
            all_children: all_children_ids,
            children: children_ids,
        }
    } else if room.is_call() {
        RoomKind::VoiceChannel
    } else {
        RoomKind::TextChannel
    };

    Some(node_from_room(room, room_kind).await)
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
    use matrix_sdk::ruma::events::AnySyncTimelineEvent;
    use matrix_sdk::sync::State;

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

pub async fn handle_room_updates(
    room_updates: &RoomUpdates,
    client: &Client,
    handle: &AppHandle,
    own_id: &str,
) {
    if should_sidebar_update(room_updates, own_id) {
        log::debug!("Refreshing sidebar");
        if let Err(e) = send_sidebar(&client.joined_rooms(), handle, own_id).await {
            log::error!("Failed to send sidebar update: {:?}", e);
        }
    }

    if should_notification_update(room_updates) {
        let counts = get_notification_counts(client).await;
        if let Err(e) = send_notification_counts_update(handle, counts) {
            log::error!("Failed to emit notification counts: {:?}", e);
        }
    }

    if let Some(call_member_updates) = extract_call_member_updates(room_updates, client).await
        && let Err(e) = send_call_member_updates(handle, call_member_updates)
    {
        log::error!("Failed to emit call member updates: {:?}", e);
    }
}

pub async fn get_notification_counts(client: &Client) -> HashMap<String, NotificationCounts> {
    client
        .rooms()
        .iter()
        .map(|room| {
            let notification_count = room.num_unread_notifications();
            let highlight_count = room.num_unread_mentions();

            (
                room.room_id().to_string(),
                NotificationCounts {
                    notification_count,
                    highlight_count,
                },
            )
        })
        .collect()
}

/// Detects changes of call members and emits the update, for all rooms grouped by room_id.
/// Reads full current state from the room on any CallMember event so leaves are handled correctly.
pub async fn extract_call_member_updates(
    room_updates: &RoomUpdates,
    client: &Client,
) -> Option<HashMap<String, Vec<UserDevice>>> {
    let mut updates = HashMap::new();

    for (room_id, update) in &room_updates.joined {
        let has_call_member_event = update.timeline.events.iter().any(|raw_event| {
            matches!(
                raw_event.raw().deserialize(),
                Ok(AnySyncTimelineEvent::State(AnySyncStateEvent::CallMember(_)))
            )
        });

        if !has_call_member_event {
            continue;
        }

        let Some(room) = client.get_room(room_id) else {
            continue;
        };

        let mut user_devices = Vec::new();
        let events = room
            .get_state_events_static::<CallMemberEventContent>()
            .await
            .unwrap_or_default();

        for raw in events {
            let Ok(SyncOrStrippedState::Sync(ev)) = raw.deserialize() else {
                continue;
            };

            let Some(OriginalSyncStateEvent {
                content: CallMemberEventContent::SessionContent(content),
                sender,
                ..
            }) = ev.as_original()
            else {
                continue;
            };

            user_devices.push(UserDevice {
                user_id: sender.to_string(),
                device_id: content.device_id.to_string(),
            });
        }

        // Always insert, even when empty — an empty vec signals that everyone left this room.
        updates.insert(room_id.to_string(), user_devices);
    }

    (!updates.is_empty()).then_some(updates)
}

pub async fn extract_call_memberships(rooms: &[Room]) -> Option<HashMap<String, Vec<UserDevice>>> {
    let mut memberships = HashMap::new();

    for room in rooms {
        if !room.is_call() {
            continue;
        }

        let mut user_devices = Vec::new();
        let events = room
            .get_state_events_static::<CallMemberEventContent>()
            .await
            .unwrap_or_default();

        for raw in events {
            let Ok(SyncOrStrippedState::Sync(ev)) = raw.deserialize() else {
                continue;
            };

            let Some(OriginalSyncStateEvent {
                content: CallMemberEventContent::SessionContent(content),
                sender,
                ..
            }) = ev.as_original()
            else {
                continue;
            };

            user_devices.push(UserDevice {
                user_id: sender.to_string(),
                device_id: content.device_id.to_string(),
            });
        }

        if !user_devices.is_empty() {
            memberships.insert(room.room_id().to_string(), user_devices);
        }
    }

    (!memberships.is_empty()).then_some(memberships)
}

pub fn send_notification_counts_update(
    handle: &AppHandle,
    counts: HashMap<String, NotificationCounts>,
) -> Result<(), TauriError> {
    handle.emit("notification_counts_update", counts)?;
    Ok(())
}

pub fn send_call_member_updates(
    handle: &AppHandle,
    update: HashMap<String, Vec<UserDevice>>,
) -> Result<(), TauriError> {
    handle.emit("call_member_update", update)?;
    Ok(())
}
