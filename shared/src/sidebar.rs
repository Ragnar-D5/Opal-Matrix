use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub enum RoomKind {
    Space {
        children: Vec<RoomNode>,
        user_ids_in_calls: Vec<String>,
    },
    TextChannel {
        last_ts: Option<u64>,
    },
    VoiceChannel {
        joined_user_ids: Vec<String>,
    },
    Dm {
        other_user_ids: Vec<String>,
        last_ts: Option<u64>,
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
    pub fn last_ts(&self) -> Option<u64> {
        match self.kind {
            RoomKind::Space { .. } => None,
            RoomKind::TextChannel { last_ts, .. } => last_ts,
            RoomKind::Dm { last_ts, .. } => last_ts,
            RoomKind::VoiceChannel { .. } => None,
        }
    }

    pub fn get_name(&self) -> String {
        self.name.clone().unwrap_or(self.room_id.clone())
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
