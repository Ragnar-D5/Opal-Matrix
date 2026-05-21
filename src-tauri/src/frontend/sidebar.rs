use std::collections::{HashMap, HashSet};

use futures::StreamExt;
use matrix_sdk::Room;

use matrix_sdk::room::ParentSpace;
use matrix_sdk::ruma::UserId;
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
            && let Some(node) = build_async_node(room_id, &channels, &parent_to_children).await
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
    parent_to_children: &HashMap<String, Vec<String>>,
) -> Option<RoomNode> {
    let room = channels.get(room_id)?;
    let unread_counts = room.unread_notification_counts();

    let mut highlight_count = unread_counts.highlight_count;
    let mut notification_count = unread_counts.notification_count;

    let room_kind = if room.is_space() {
        let mut children_nodes = Vec::new();

        if let Some(child_ids) = parent_to_children.get(room_id) {
            for child_id in child_ids {
                if let Some(child_node) =
                    build_async_node(child_id, channels, parent_to_children).await
                {
                    highlight_count += child_node.highlight_count;
                    notification_count += child_node.notification_count;
                    children_nodes.push(child_node);
                }
            }
        }

        RoomKind::Space {
            children: children_nodes,
        }
    } else {
        RoomKind::TextChannel {
            last_ts: room.latest_event_timestamp().map(|t| t.as_secs().into()),
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
