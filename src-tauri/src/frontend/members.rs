use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct UserProfile {
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}
