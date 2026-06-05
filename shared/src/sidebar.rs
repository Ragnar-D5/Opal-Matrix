use std::collections::HashSet;

use colorsys::Hsl;
use serde::{Deserialize, Serialize};

use crate::get_color;

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct UserDevice {
    pub user_id: String,
    pub device_id: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub enum RoomKind {
    Space {
        children: Vec<RoomNode>,
        all_children: HashSet<String>,
    },
    TextChannel,
    VoiceChannel,
    Dm {
        other_user_id: String,
    },
    // GroupDm {
    // other_user_ids: Vec<String>,
    // },
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct RoomNode {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub has_avatar: bool,

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

    pub fn avatar_url(&self) -> Option<String> {
        if self.has_avatar {
            match &self.kind {
                RoomKind::Dm { other_user_id } => Some(format!(
                    "mxc://user/{}/room/{}",
                    other_user_id, self.room_id
                )),
                _ => Some(format!("mxc://room/{}", self.room_id)),
            }
        } else {
            None
        }
    }

    pub fn get_color(&self) -> Hsl {
        if let RoomKind::Dm { other_user_id } = &self.kind {
            get_color(other_user_id)
        } else {
            get_color(&self.room_id)
        }
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
