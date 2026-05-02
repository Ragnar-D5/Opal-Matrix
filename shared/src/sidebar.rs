use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub enum RoomKind {
    Space { children: Vec<RoomNode> },
    Channel { last_ts: Option<i64> },
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct RoomNode {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,

    pub highlight_count: u32,
    pub notification_count: u32,

    pub kind: RoomKind,
}

impl RoomNode {
    pub fn last_ts(&self) -> Option<i64> {
        match self.kind {
            RoomKind::Space { .. } => None,
            RoomKind::Channel { last_ts, .. } => last_ts,
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
    pub highlight_count: u32,
    pub notification_count: u32,
}
