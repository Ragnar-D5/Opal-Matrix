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
    pub avatar_url: Option<String>,
}

impl UserProfile {
    pub fn get_name(&self) -> String {
        self.display_name
            .as_ref()
            .cloned()
            .unwrap_or_else(|| self.user_id.clone())
    }
}
