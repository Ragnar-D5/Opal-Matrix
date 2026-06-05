use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum PresenceStatus {
    Online,
    #[default]
    Offline,
    Unavailable,
    Busy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PresenceInfo {
    pub status: PresenceStatus,
    pub status_msg: Option<String>,
    pub last_active_ago: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UserProfile {
    pub user_id: String,
    pub display_name: Option<String>,

    pub has_avatar: bool,
}

impl UserProfile {
    pub fn get_name(&self) -> String {
        self.display_name
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.user_id.clone())
    }

    pub fn get_avatar_url(&self, room_id: &str) -> String {
        format!("mxc://user/{}/room/{room_id}", self.user_id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MemberProfile {
    pub room_id: String,
    pub profile: UserProfile,
}

impl MemberProfile {
    pub fn get_name(&self) -> String {
        self.profile.get_name()
    }

    pub fn get_avatar_url(&self) -> String {
        self.profile.get_avatar_url(&self.room_id)
    }

    pub fn user_id(&self) -> &str {
        &self.profile.user_id
    }
}
