use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use futures::StreamExt;
use matrix_sdk::{Client, Room};

use matrix_sdk::room::ParentSpace;
use matrix_sdk::ruma::UserId;
use matrix_sdk::ruma::events::space::child::SpaceChildEventContent;
use matrix_sdk::sync::RoomUpdates;
use shared::sidebar::{RoomKind, RoomNode, SidebarState};
use tauri::AppHandle;
use tauri::Emitter;

use crate::TauriError;

pub async fn send_sidebar(all_rooms: &[Room], handle: &AppHandle) -> Result<(), TauriError> {
    let mut channels = HashMap::new();
    let mut dms = Vec::new();

    for room in all_rooms.iter() {
        if room.compute_is_dm().await? {
            let unread_counts = room.unread_notification_counts();

            let targets = room.direct_targets();
            let other_user_ids: Vec<&UserId> =
                targets.iter().filter_map(|u| u.as_user_id()).collect();

            let Some(first_id) = other_user_ids.first() else {
                log::warn!("DM room {} has no direct targets", room.room_id());
                continue;
            };
            let Some(first_other_user) = room.get_member(first_id).await? else {
                log::warn!("DM room {} has no valid first other user", room.room_id());
                continue;
            };

            dms.push(RoomNode {
                room_id: room.room_id().to_string(),
                name: room.display_name().await.ok().map(|n| n.to_string()),
                avatar_url: first_other_user.avatar_url().map(|u| u.to_string()),
                highlight_count: unread_counts.highlight_count,
                notification_count: unread_counts.notification_count,
                topic: room.topic(),

                kind: RoomKind::Dm {
                    other_user_ids: room
                        .direct_targets()
                        .iter()
                        .map(|u| u.to_string())
                        .collect(),
                    last_ts: room.latest_event_timestamp().map(|t| t.as_secs().into()),
                },
            });
        } else {
            channels.insert(room.room_id().to_string(), room);
        }
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
            let unread_counts = room.unread_notification_counts();
            orphaned_channels.push(RoomNode {
                room_id: room_id.clone(),
                name: room.display_name().await.ok().map(|n| n.to_string()),
                topic: room.topic(),
                avatar_url: room.avatar_url().map(|u| u.to_string()),
                highlight_count: unread_counts.highlight_count,
                notification_count: unread_counts.notification_count,
                kind: RoomKind::TextChannel {
                    last_ts: room.latest_event_timestamp().map(|t| t.as_secs().into()),
                },
            });
        }
    }

    dms.sort_by(|a, b| {
        let ts = |n: &RoomNode| match &n.kind {
            RoomKind::Dm { last_ts, .. } => last_ts.unwrap_or(0),
            _ => 0,
        };
        ts(b).cmp(&ts(a))
    });

    let sidebar_state = SidebarState {
        dms,
        servers: top_level_servers,
        orphaned_rooms: orphaned_channels,
    };

    handle.emit("sidebar_update", sidebar_state)?;

    Ok(())
}

use async_recursion::async_recursion;

#[async_recursion]
async fn build_async_node(
    room_id: &str,
    channels: &HashMap<String, &Room>,
    parent_to_children: &HashMap<String, Vec<(String, Option<String>)>>,
) -> Option<RoomNode> {
    let room = channels.get(room_id)?;
    let unread_counts = room.unread_notification_counts();

    let mut highlight_count = unread_counts.highlight_count;
    let mut notification_count = unread_counts.notification_count;
    let mut user_ids_in_calls = HashSet::new();

    let room_kind = if room.is_space() {
        let mut children_nodes = Vec::new();

        if let Some(child_ids) = parent_to_children.get(room_id) {
            for (child_id, _) in child_ids {
                if let Some(child_node) =
                    build_async_node(child_id, channels, parent_to_children).await
                {
                    highlight_count += child_node.highlight_count;
                    notification_count += child_node.notification_count;
                    if let RoomKind::VoiceChannel { joined_user_ids } = &child_node.kind {
                        for user_id in joined_user_ids {
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
            let joined_user_ids = room
                .active_room_call_participants()
                .iter()
                .map(|v| v.to_string())
                .collect();
            RoomKind::VoiceChannel { joined_user_ids }
        } else {
            RoomKind::TextChannel {
                last_ts: room.latest_event_timestamp().map(|t| t.as_secs().into()),
            }
        }
    };

    Some(RoomNode {
        room_id: room_id.to_string(),
        name: room.display_name().await.ok().map(|n| n.to_string()),
        topic: room.topic(),
        avatar_url: room.avatar_url().map(|u| u.to_string()),
        highlight_count,
        notification_count,
        kind: room_kind,
    })
}

fn should_sidebar_update(room_updates: &RoomUpdates) -> bool {
    if !room_updates.left.is_empty() || !room_updates.invited.is_empty() {
        return true;
    }

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
                && (event_type == "m.direct" || event_type == "m.fully_read")
            {
                return true;
            }
        }
    }

    false
}

pub async fn handle_room_updates(room_updates: &RoomUpdates, client: &Client, handle: &AppHandle) {
    if should_sidebar_update(room_updates) {
        log::debug!("Significant room update detected, refreshing sidebar");
        if let Err(e) = send_sidebar(&client.joined_rooms(), handle).await {
            log::error!("Failed to send sidebar update: {:?}", e);
        }
    }
}
