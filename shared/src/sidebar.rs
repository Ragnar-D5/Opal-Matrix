use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Maps user_id → list of device_ids currently in the call.
pub type VoiceParticipants = HashMap<String, Vec<String>>;

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub enum RoomKind {
    Space {
        children: Vec<RoomNode>,
        user_ids_in_calls: Vec<String>,
    },
    TextChannel,
    VoiceChannel {
        participants: VoiceParticipants,
    },
    Dm {
        other_user_id: String,
    },
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct RoomNode {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,

    pub kind: RoomKind,
}

impl RoomNode {
    pub fn get_name(&self) -> String {
        self.name.clone().unwrap_or(self.room_id.clone())
    }

    pub fn dm_user_id(&self) -> Option<String> {
        if let RoomKind::Dm { other_user_id, .. } = &self.kind {
            return Some(other_user_id.clone());
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

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Default)]
pub struct NotificationCounts {
    pub highlight_count: u64,
    pub notification_count: u64,
}

impl NotificationCounts {
    pub fn has_notifications(&self) -> bool {
        self.notification_count > 0
    }
}

pub trait MaybeNotificationCounts {
    fn has_notifications(&self) -> bool;
}

impl MaybeNotificationCounts for Option<NotificationCounts> {
    fn has_notifications(&self) -> bool {
        if let Some(counts) = self {
            counts.has_notifications()
        } else {
            false
        }
    }
}
