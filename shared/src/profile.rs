use csscolorparser::Color;
use macros::TauriEvent;
use ruma::{OwnedRoomId, OwnedUserId, RoomId, UserId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::synth::SonicSignature;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum PresenceStatus {
    Online,
    #[default]
    Offline,
    Unavailable,
    Busy,
}

impl std::fmt::Display for PresenceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PresenceStatus::Online => "Online",
                PresenceStatus::Offline => "Offline",
                PresenceStatus::Unavailable => "Unavailable",
                PresenceStatus::Busy => "Busy",
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, TauriEvent)]
pub struct PresenceInfo {
    pub status: PresenceStatus,
    pub status_msg: Option<String>,
    pub last_active_ago: Option<u64>,
}

impl PresenceInfo {
    pub fn is_offline(&self) -> bool {
        matches!(self.status, PresenceStatus::Offline)
    }

    pub fn new_online() -> Self {
        Self {
            status: PresenceStatus::Online,
            status_msg: None,
            last_active_ago: None,
        }
    }

    pub fn new_offline() -> Self {
        Self {
            status: PresenceStatus::Offline,
            status_msg: None,
            last_active_ago: None,
        }
    }

    pub fn presence_message(&self) -> String {
        self.status_msg
            .clone()
            .unwrap_or_else(|| self.status.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TauriEvent)]
pub struct UserProfile {
    pub user_id: OwnedUserId,
    pub display_name: Option<String>,

    pub has_avatar: bool,

    pub custom_properties: CustomProperties,
}

impl UserProfile {
    pub fn get_name(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| self.user_id.to_string())
    }

    pub fn get_avatar_url(&self, room_id: &RoomId) -> String {
        format!("mxc://user/{}/room/{room_id}", self.user_id)
    }

    pub fn get_signature(&self) -> SonicSignature {
        self.custom_properties.sonic_signature.clone()
    }

    pub fn name_color(&self) -> Color {
        self.custom_properties.name_color.clone()
    }

    pub fn banner_color(&self) -> Color {
        self.custom_properties.banner_color.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TauriEvent)]
pub struct MemberProfile {
    pub room_id: OwnedRoomId,
    pub profile: UserProfile,
}

impl MemberProfile {
    pub fn get_name(&self) -> String {
        self.profile.get_name()
    }

    pub fn get_avatar_url(&self) -> String {
        self.profile.get_avatar_url(&self.room_id)
    }

    pub fn user_id(&self) -> OwnedUserId {
        self.profile.user_id.clone()
    }

    pub fn get_signature(&self) -> SonicSignature {
        self.profile.get_signature()
    }

    pub fn name_color(&self) -> Color {
        self.profile.name_color()
    }

    pub fn banner_color(&self) -> Color {
        self.profile.banner_color()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomProfile {
    pub room_id: OwnedRoomId,
    pub aliases: Vec<String>,
    pub canonical_alias: Option<String>,
    pub name: Option<String>,
}

impl RoomProfile {
    pub fn get_name(&self) -> String {
        self.name
            .clone()
            .map(|n| format!("#{n}"))
            .or_else(|| self.canonical_alias.clone())
            .or_else(|| self.aliases.first().cloned())
            .unwrap_or_else(|| self.room_id.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomProperties {
    pub banner_color: Color,
    pub name_color: Color,
    pub sonic_signature: SonicSignature,
}

impl CustomProperties {
    pub fn from_user_id(user_id: &UserId) -> Self {
        let hash = Sha256::digest(user_id.as_bytes());
        let h = hash[0] as f32 / 255.0 * 360.0;
        let color = Color::from_hsla(h, 0.9, 0.7, 1.0);

        Self {
            banner_color: color.clone(),
            name_color: color,
            sonic_signature: SonicSignature::from_string(user_id.as_ref()),
        }
    }
}
