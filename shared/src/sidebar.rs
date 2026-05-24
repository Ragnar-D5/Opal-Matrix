use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub enum RoomKind {
    Space {
        children: Vec<RoomNode>,
    },
    TextChannel {
        last_ts: Option<i64>,
    },
    VoiceChannel {
        joined_user_ids: Vec<String>,
    },
    Dm {
        other_user_ids: Vec<String>,
        last_ts: Option<i64>,
    },
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct RoomNode {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,

    pub highlight_count: u64,
    pub notification_count: u64,

    pub kind: RoomKind,
}

impl RoomNode {
    pub fn last_ts(&self) -> Option<i64> {
        match self.kind {
            RoomKind::Space { .. } => None,
            RoomKind::TextChannel { last_ts, .. } => last_ts,
            RoomKind::Dm { last_ts, .. } => last_ts,
            RoomKind::VoiceChannel { .. } => None,
        }
    }

    pub fn get_name(&self) -> String {
        if let Some(name) = &self.name {
            return name.clone();
        }

        self.room_id.clone()
    }

    pub fn dm_user_id(&self) -> Option<String> {
        if let RoomKind::Dm { other_user_ids, .. } = &self.kind {
            return other_user_ids.first().cloned();
        }

        None
    }
}

#[derive(Debug, Serialize, Clone, Default, Deserialize, PartialEq)]
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
    pub highlight_count: u64,
    pub notification_count: u64,
}
