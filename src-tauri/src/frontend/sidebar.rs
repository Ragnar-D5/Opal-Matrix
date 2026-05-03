use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use shared::sidebar::{FlatRoom, RoomKind, RoomNode, SidebarState};

use crate::storage::members::get_other_member_in_dm;

pub fn build_tree(
    conn: &Connection,
    own_id: &String,
    mut all_rooms: HashMap<String, FlatRoom>,
    parent_to_children: HashMap<String, Vec<String>>,
    all_children: HashSet<String>,
) -> SidebarState {
    let mut dms = Vec::new();
    let mut top_level_servers = Vec::new();

    // Extract DMs and remove them from the main rooms list
    all_rooms.retain(|room_id, room| {
        if room.is_direct {
            let dm_user_id = get_other_member_in_dm(conn, room_id, own_id);

            dms.push(RoomNode {
                room_id: room_id.clone(),
                name: room.name.clone(),
                topic: room.topic.clone(),
                avatar_url: room.avatar_url.clone(),

                dm_user_id: dm_user_id,

                highlight_count: room.highlight_count,
                notification_count: room.notification_count,

                kind: RoomKind::Channel {
                    last_ts: room.last_ts,
                },
            });
            false
        } else {
            true
        }
    });

    // Find top-level servers
    for (room_id, room) in &all_rooms {
        if room.room_type.as_deref() == Some("m.space") && !all_children.contains(room_id) {
            if let Some(node) = build_node(room_id, &all_rooms, &parent_to_children) {
                top_level_servers.push(node);
            }
        }
    }

    // Find orphaned channels (those that are not DMs, not spaces, and not children of any space)
    let mut orphaned_channels = Vec::new();
    for (room_id, room) in &all_rooms {
        if room.room_type.is_none() && !all_children.contains(room_id) {
            orphaned_channels.push(RoomNode {
                room_id: room_id.clone(),
                name: room.name.clone(),
                topic: room.topic.clone(),
                avatar_url: room.avatar_url.clone(),

                dm_user_id: None,

                highlight_count: room.highlight_count,
                notification_count: room.notification_count,

                kind: RoomKind::Channel {
                    last_ts: room.last_ts,
                },
            });
        }
    }

    SidebarState {
        dms: dms,
        servers: top_level_servers,
        orphaned_rooms: orphaned_channels,
    }
}

fn build_node(
    room_id: &str,
    all_rooms: &HashMap<String, FlatRoom>,
    parent_to_children: &HashMap<String, Vec<String>>,
) -> Option<RoomNode> {
    let room = all_rooms.get(room_id)?;

    let mut highlight_count = room.highlight_count;
    let mut notification_count = room.notification_count;

    let room_kind = if room.room_type.as_deref() == Some("m.space") {
        let mut children_nodes = Vec::new();

        if let Some(child_ids) = parent_to_children.get(room_id) {
            for child_id in child_ids {
                if let Some(child_node) = build_node(child_id, all_rooms, parent_to_children) {
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
        RoomKind::Channel {
            last_ts: room.last_ts,
        }
    };

    return Some(RoomNode {
        room_id: room_id.to_string(),
        name: room.name.clone(),
        topic: room.topic.clone(),
        avatar_url: room.avatar_url.clone(),

        dm_user_id: None,

        highlight_count: highlight_count,
        notification_count: notification_count,

        kind: room_kind,
    });
}
