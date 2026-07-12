use std::collections::HashMap;

use csscolorparser::Color;
use macros::TauriEvent;
use serde::{Deserialize, Serialize};

use crate::{account_data::ServerOrder, profile::RoomProfile};

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, TauriEvent)]
pub struct UserDevice {
    pub user_id: String,
    pub device_id: String,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct SpaceRoomNode {
    pub info: RoomNodeInfo,
    pub children: Vec<String>,
}

impl SpaceRoomNode {
    pub fn name(&self) -> String {
        self.info.name.clone()
    }
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct ServerRoomNode {
    pub info: RoomNodeInfo,
    pub children: Vec<String>,
    /// All children of the server, recursively, including children of children.
    pub all_children: Vec<String>,
}

impl ServerRoomNode {
    pub fn name(&self) -> String {
        self.info.name.clone()
    }

    pub fn room_id(&self) -> String {
        self.info.room_id.clone()
    }
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct TextChannelRoomNode {
    pub info: RoomNodeInfo,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct VoiceChannelRoomNode {
    pub info: RoomNodeInfo,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct DmRoomNode {
    pub info: RoomNodeInfo,
    pub other_user_id: String,
}

impl DmRoomNode {
    pub fn name(&self) -> String {
        self.info.name.clone()
    }

    pub fn room_id(&self) -> String {
        self.info.room_id.clone()
    }

    pub fn avatar_url(&self) -> Option<String> {
        self.info.has_avatar.then_some(format!(
            "mxc://user/{}/room/{}",
            self.other_user_id, self.info.room_id
        ))
    }
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct SingleRoomNode {
    pub info: RoomNodeInfo,
    pub other_user_ids: Vec<String>,
}

impl SingleRoomNode {
    pub fn name(&self) -> String {
        self.info.name.clone()
    }

    pub fn room_id(&self) -> String {
        self.info.room_id.clone()
    }
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct UnjoinedRoomNode {
    pub info: RoomNodeInfo,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, TauriEvent)]
pub enum RoomNode {
    Space(SpaceRoomNode),
    Server(ServerRoomNode),
    TextChannel(TextChannelRoomNode),
    VoiceChannel(VoiceChannelRoomNode),
    Dm(DmRoomNode),
    Single(SingleRoomNode),
    Unjoined(UnjoinedRoomNode),
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Default)]
pub struct RoomRights {
    pub send_messages: bool,
    pub send_reactions: bool,
    pub mention_everyone: bool,

    pub pin_messages: bool,
    pub delete_messages: bool,
    pub kick_users: bool,
    pub ban_users: bool,
    pub invite_users: bool,

    pub change_name_and_avatar: bool,
    pub change_topic: bool,
    pub manage_permissions: bool,

    pub manage_children: bool,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct RoomNodeInfo {
    pub room_id: String,
    pub name: String,
    pub topic: Option<String>,
    pub has_avatar: bool,

    pub rights: RoomRights,

    pub color: Color,

    pub canonical_alias: Option<String>,
    pub aliases: Vec<String>,
}

impl RoomNodeInfo {
    pub fn avatar_url(&self) -> Option<String> {
        self.has_avatar
            .then_some(format!("mxc://room/{}", self.room_id))
    }
}

impl RoomNode {
    pub fn info(&self) -> RoomNodeInfo {
        match self {
            RoomNode::Space(node) => node.info.clone(),
            RoomNode::TextChannel(node) => node.info.clone(),
            RoomNode::VoiceChannel(node) => node.info.clone(),
            RoomNode::Dm(node) => node.info.clone(),
            RoomNode::Server(node) => node.info.clone(),
            RoomNode::Single(node) => node.info.clone(),
            RoomNode::Unjoined(node) => node.info.clone(),
        }
    }

    pub fn name(&self) -> String {
        match self {
            RoomNode::Space(node) => node.info.name.clone(),
            RoomNode::TextChannel(node) => node.info.name.clone(),
            RoomNode::VoiceChannel(node) => node.info.name.clone(),
            RoomNode::Dm(node) => node.info.name.clone(),
            RoomNode::Server(node) => node.info.name.clone(),
            RoomNode::Single(node) => node.info.name.clone(),
            RoomNode::Unjoined(node) => node.info.name.clone(),
        }
    }

    pub fn room_id(&self) -> String {
        match self {
            RoomNode::Space(node) => node.info.room_id.clone(),
            RoomNode::TextChannel(node) => node.info.room_id.clone(),
            RoomNode::VoiceChannel(node) => node.info.room_id.clone(),
            RoomNode::Dm(node) => node.info.room_id.clone(),
            RoomNode::Server(node) => node.info.room_id.clone(),
            RoomNode::Single(node) => node.info.room_id.clone(),
            RoomNode::Unjoined(node) => node.info.room_id.clone(),
        }
    }

    fn canonical_alias(&self) -> Option<String> {
        match self {
            RoomNode::Space(node) => node.info.canonical_alias.clone(),
            RoomNode::TextChannel(node) => node.info.canonical_alias.clone(),
            RoomNode::VoiceChannel(node) => node.info.canonical_alias.clone(),
            RoomNode::Dm(node) => node.info.canonical_alias.clone(),
            RoomNode::Server(node) => node.info.canonical_alias.clone(),
            RoomNode::Single(node) => node.info.canonical_alias.clone(),
            RoomNode::Unjoined(node) => node.info.canonical_alias.clone(),
        }
    }

    fn aliases(&self) -> Vec<String> {
        match self {
            RoomNode::Space(node) => node.info.aliases.clone(),
            RoomNode::TextChannel(node) => node.info.aliases.clone(),
            RoomNode::VoiceChannel(node) => node.info.aliases.clone(),
            RoomNode::Dm(node) => node.info.aliases.clone(),
            RoomNode::Server(node) => node.info.aliases.clone(),
            RoomNode::Single(node) => node.info.aliases.clone(),
            RoomNode::Unjoined(node) => node.info.aliases.clone(),
        }
    }

    fn has_avatar(&self) -> bool {
        match self {
            RoomNode::Space(node) => node.info.has_avatar,
            RoomNode::TextChannel(node) => node.info.has_avatar,
            RoomNode::VoiceChannel(node) => node.info.has_avatar,
            RoomNode::Dm(node) => node.info.has_avatar,
            RoomNode::Server(node) => node.info.has_avatar,
            RoomNode::Single(node) => node.info.has_avatar,
            RoomNode::Unjoined(node) => node.info.has_avatar,
        }
    }

    pub fn avatar_url(&self) -> Option<String> {
        if let RoomNode::Unjoined(node) = self {
            return node.avatar_url.clone();
        }

        self.has_avatar().then_some(
            if let RoomNode::Dm(DmRoomNode { other_user_id, .. }) = self {
                format!("mxc://user/{}/room/{}", self.room_id(), other_user_id)
            } else {
                format!("mxc://room/{}", self.room_id())
            },
        )
    }

    pub fn color(&self) -> Color {
        match self {
            RoomNode::Space(node) => node.info.color.clone(),
            RoomNode::TextChannel(node) => node.info.color.clone(),
            RoomNode::VoiceChannel(node) => node.info.color.clone(),
            RoomNode::Dm(node) => node.info.color.clone(),
            RoomNode::Server(node) => node.info.color.clone(),
            RoomNode::Single(node) => node.info.color.clone(),
            RoomNode::Unjoined(node) => node.info.color.clone(),
        }
    }

    pub fn as_dm(&self) -> Option<DmRoomNode> {
        if let RoomNode::Dm(dm_node) = self {
            Some(dm_node.clone())
        } else {
            None
        }
    }

    pub fn is_dm(&self) -> bool {
        matches!(self, RoomNode::Dm(_))
    }

    pub fn as_server(&self) -> Option<ServerRoomNode> {
        if let RoomNode::Server(server_node) = self {
            Some(server_node.clone())
        } else {
            None
        }
    }

    pub fn as_single(&self) -> Option<SingleRoomNode> {
        if let RoomNode::Single(single_node) = self {
            Some(single_node.clone())
        } else {
            None
        }
    }

    pub fn is_unjoined(&self) -> bool {
        matches!(self, RoomNode::Unjoined(_))
    }

    pub fn is_space(&self) -> bool {
        matches!(self, RoomNode::Space(_) | RoomNode::Server(_))
    }
}

impl From<RoomNode> for RoomProfile {
    fn from(node: RoomNode) -> Self {
        RoomProfile {
            room_id: node.room_id().to_string(),
            name: Some(node.name()),
            canonical_alias: node.canonical_alias(),
            aliases: node.aliases(),
        }
    }
}

#[derive(Debug, Serialize, Clone, Default, Deserialize, PartialEq, TauriEvent)]
pub struct ServerList(pub Vec<String>);

impl ServerList {
    pub fn reorder_servers(&self, source_id: &str, target_id: &str) -> Self {
        let source_index = self.0.iter().position(|id| id == source_id);
        let target_index = self.0.iter().position(|id| id == target_id);

        let mut clone = self.0.clone();
        if let (Some(source_index), Some(target_index)) = (source_index, target_index) {
            clone.remove(source_index);
            clone.insert(target_index, source_id.to_string());
        }

        Self(clone)
    }

    pub fn apply_order(&self, order: ServerOrder) -> Self {
        let new_order: Vec<String> = order
            .servers
            .iter()
            .filter(|id| self.0.contains(id))
            .cloned()
            .collect();

        Self(new_order)
    }
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, TauriEvent, Default)]
pub struct DmList(pub Vec<String>);

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, TauriEvent, Default)]
pub struct SingleList(pub Vec<String>);

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, TauriEvent)]
pub enum RoomMapUpdate {
    Insert { key: String, value: RoomNode },
    Remove { key: String },
    Set { map: HashMap<String, RoomNode> },
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Default, TauriEvent)]
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
