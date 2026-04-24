use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Deserialize)]
pub enum RoomNode {
    Space {
        room_id: String,
        name: Option<String>,
        topic: Option<String>,
        avatar_url: Option<String>,

        children: Vec<RoomNode>,
    },
    Channel {
        room_id: String,
        name: Option<String>,
        topic: Option<String>,
        avatar_url: Option<String>,

        last_ts: Option<i64>,
    },
}

impl RoomNode {
    pub fn id(&self) -> &str {
        match self {
            RoomNode::Space { room_id, .. } => room_id,
            RoomNode::Channel { room_id, .. } => room_id,
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            RoomNode::Space { name, .. } | RoomNode::Channel { name, .. } => {
                name.clone().unwrap_or_else(|| "Unnamed".to_string())
            }
        }
    }

    pub fn last_ts(&self) -> Option<i64> {
        match self {
            RoomNode::Space { .. } => None,
            RoomNode::Channel { last_ts, .. } => *last_ts,
        }
    }

    pub fn avatar_url(&self) -> Option<String> {
        match self {
            RoomNode::Space { avatar_url, .. } | RoomNode::Channel { avatar_url, .. } => {
                avatar_url.clone()
            }
        }
    }
}

#[derive(Debug, Serialize, Clone, Default, Deserialize)]
pub struct SidebarState {
    pub dms: Vec<RoomNode>,
    pub servers: Vec<RoomNode>,
    pub orphaned_rooms: Vec<RoomNode>,
}

pub struct FlatRoom {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub room_type: Option<String>,
    pub is_direct: bool,
    pub last_ts: Option<i64>,
}
