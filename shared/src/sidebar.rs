use std::collections::{HashMap, HashSet};

use colorsys::Hsl;
use serde::{Deserialize, Serialize};

use crate::{account_data::ServerOrder, get_color};

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct UserDevice {
    pub user_id: String,
    pub device_id: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Default)]
pub enum RoomKind {
    Space {
        children: Vec<String>,
        all_children: HashSet<String>,
    },
    #[default]
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

    pub canonical_alias: Option<String>,
    pub aliases: Vec<String>,

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
    pub top_level_servers: Vec<String>,
    pub orphaned_rooms: Vec<RoomNode>,

    /// Rooms that aren't DMs or orphaned (not in a space)
    pub server_rooms: HashMap<String, RoomNode>,
}

impl SidebarState {
    pub fn reorder_servers(&self, source_id: &str, target_id: &str) -> Self {
        let mut new_state = self.clone();

        let source_index = new_state
            .top_level_servers
            .iter()
            .position(|id| id == source_id);
        let target_index = new_state
            .top_level_servers
            .iter()
            .position(|id| id == target_id);

        if let (Some(source_index), Some(target_index)) = (source_index, target_index) {
            new_state.top_level_servers.remove(source_index);
            new_state
                .top_level_servers
                .insert(target_index, source_id.to_string());
        }

        new_state
    }

    pub fn apply_order(&self, order: ServerOrder) -> Self {
        let new_order: Vec<String> = order
            .servers
            .iter()
            .filter(|id| self.top_level_servers.contains(id))
            .cloned()
            .collect();

        Self {
            top_level_servers: new_order,
            ..self.clone()
        }
    }
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
