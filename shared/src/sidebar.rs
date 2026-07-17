use std::{collections::HashMap, ops::AddAssign};

use csscolorparser::Color;
use macros::TauriEvent;
use ruma::{
    OwnedDeviceId, OwnedMxcUri, OwnedRoomId, OwnedUserId, RoomId,
    events::room::history_visibility::HistoryVisibility, room::JoinRuleSummary,
};
use serde::{Deserialize, Serialize};

use crate::{account_data::ServerOrder, profile::RoomProfile};

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, TauriEvent)]
pub struct UserDevice {
    pub user_id: OwnedUserId,
    pub device_id: OwnedDeviceId,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct SpaceRoomNode {
    pub info: RoomNodeInfo,
    pub children: Vec<OwnedRoomId>,
}

impl SpaceRoomNode {
    pub fn name(&self) -> String {
        self.info.name.clone()
    }
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq)]
pub struct ServerRoomNode {
    pub info: RoomNodeInfo,
    pub children: Vec<OwnedRoomId>,
    /// All children of the server, recursively, including children of children.
    pub all_children: Vec<OwnedRoomId>,
}

impl ServerRoomNode {
    pub fn name(&self) -> String {
        self.info.name.clone()
    }

    pub fn room_id(&self) -> OwnedRoomId {
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
    pub other_user_id: OwnedUserId,
}

impl DmRoomNode {
    pub fn name(&self) -> String {
        self.info.name.clone()
    }

    pub fn room_id(&self) -> OwnedRoomId {
        self.info.room_id.clone()
    }

    pub fn avatar_url(&self) -> Option<OwnedMxcUri> {
        self.info.avatar_url.clone()
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

    pub fn room_id(&self) -> OwnedRoomId {
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
    pub room_id: OwnedRoomId,
    pub name: String,
    pub topic: Option<String>,

    pub avatar_url: Option<OwnedMxcUri>,

    pub rights: RoomRights,

    pub color: Color,

    pub canonical_alias: Option<String>,
    pub aliases: Vec<String>,
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
        self.info().name.clone()
    }

    pub fn room_id(&self) -> OwnedRoomId {
        self.info().room_id.clone()
    }

    fn canonical_alias(&self) -> Option<String> {
        self.info().canonical_alias.clone()
    }

    fn aliases(&self) -> Vec<String> {
        self.info().aliases.clone()
    }

    pub fn avatar_url(&self) -> Option<OwnedMxcUri> {
        self.info().avatar_url.clone()
    }

    pub fn color(&self) -> Color {
        self.info().color.clone()
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

    pub fn has_children(&self) -> bool {
        self.children().is_some_and(|children| !children.is_empty())
    }

    pub fn children(&self) -> Option<Vec<OwnedRoomId>> {
        match self {
            RoomNode::Space(space_node) => Some(space_node.children.clone()),
            RoomNode::Server(server_node) => Some(server_node.children.clone()),
            _ => None,
        }
    }

    pub fn icon_character(&self) -> char {
        match self {
            RoomNode::Dm(_) => '@',
            _ => '#',
        }
    }

    pub fn text_signature(&self) -> String {
        format!("{}{}", self.icon_character(), self.name())
    }
}

impl From<RoomNode> for RoomProfile {
    fn from(node: RoomNode) -> Self {
        RoomProfile {
            room_id: node.room_id(),
            name: Some(node.name()),
            canonical_alias: node.canonical_alias(),
            aliases: node.aliases(),
        }
    }
}

#[derive(Debug, Serialize, Clone, Default, Deserialize, PartialEq, TauriEvent)]
pub struct ServerList(pub Vec<OwnedRoomId>);

impl ServerList {
    pub fn reorder_servers(&self, source_id: &RoomId, target_id: &RoomId) -> Self {
        let source_index = self.0.iter().position(|id| id == source_id);
        let target_index = self.0.iter().position(|id| id == target_id);

        let mut clone = self.0.clone();
        if let (Some(source_index), Some(target_index)) = (source_index, target_index) {
            clone.remove(source_index);
            clone.insert(target_index, source_id.to_owned());
        }

        Self(clone)
    }

    pub fn apply_order(&self, order: ServerOrder) -> Self {
        let new_order: Vec<OwnedRoomId> = order
            .servers
            .iter()
            .filter(|id| self.0.contains(id))
            .cloned()
            .collect();

        Self(new_order)
    }
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, TauriEvent, Default)]
pub struct DmList(pub Vec<OwnedRoomId>);

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, TauriEvent, Default)]
pub struct SingleList(pub Vec<OwnedRoomId>);

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, TauriEvent)]
pub enum RoomMapUpdate {
    Insert { key: OwnedRoomId, value: RoomNode },
    Remove { key: OwnedRoomId },
    Set { map: HashMap<OwnedRoomId, RoomNode> },
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, Default, TauriEvent)]
pub struct NotificationCounts {
    pub highlight_count: u64,
    pub notification_count: u64,
}

impl AddAssign for NotificationCounts {
    fn add_assign(&mut self, other: Self) {
        self.highlight_count += other.highlight_count;
        self.notification_count += other.notification_count;
    }
}

impl NotificationCounts {
    pub fn has_notifications(&self) -> bool {
        self.notification_count > 0 || self.highlight_count > 0
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

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, TauriEvent)]
pub enum UiMembership {
    Join,
    Invite,
    Leave,
    Ban,
    Knock,
}

#[derive(Debug, Serialize, Clone, Deserialize, PartialEq, TauriEvent)]
pub struct RoomExtraInfo {
    pub membership: UiMembership,
    pub join_rule: JoinRuleSummary,
    pub history_visibility: HistoryVisibility,
    pub encrypted: bool,
    pub version: Option<String>,
    pub num_joined_users: u64,
}
