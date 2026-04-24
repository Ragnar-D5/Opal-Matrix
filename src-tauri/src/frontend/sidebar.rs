use crate::TauriError;
use crate::storage::fetch_sidebar;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use tauri::{AppHandle, Emitter};

use shared::sidebar::{FlatRoom, RoomNode, SidebarState};

pub fn build_tree(
    mut all_rooms: HashMap<String, FlatRoom>,
    parent_to_children: HashMap<String, Vec<String>>,
    all_children: HashSet<String>,
) -> SidebarState {
    let mut dms = Vec::new();
    let mut top_level_servers = Vec::new();

    // Extract DMs and remove them from the main rooms list
    all_rooms.retain(|room_id, room| {
        if room.is_direct {
            dms.push(RoomNode::Channel {
                room_id: room_id.clone(),
                name: room.name.clone(),
                topic: room.topic.clone(),
                avatar_url: room.avatar_url.clone(),

                last_ts: room.last_ts,
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
            orphaned_channels.push(RoomNode::Channel {
                room_id: room_id.clone(),
                name: room.name.clone(),
                topic: room.topic.clone(),
                avatar_url: room.avatar_url.clone(),

                last_ts: room.last_ts,
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

    if room.room_type.as_deref() == Some("m.space") {
        let mut children_nodes = Vec::new();

        if let Some(child_ids) = parent_to_children.get(room_id) {
            for child_id in child_ids {
                if let Some(child_node) = build_node(child_id, all_rooms, parent_to_children) {
                    children_nodes.push(child_node);
                }
            }
        }

        Some(RoomNode::Space {
            room_id: room.room_id.clone(),
            name: room.name.clone(),
            topic: room.topic.clone(),
            avatar_url: room.avatar_url.clone(),
            children: children_nodes,
        })
    } else {
        Some(RoomNode::Channel {
            room_id: room_id.to_string(),
            name: room.name.clone(),
            topic: room.topic.clone(),
            avatar_url: room.avatar_url.clone(),

            last_ts: room.last_ts,
        })
    }
}

