use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
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

#[derive(Debug, Serialize, Clone)]
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
