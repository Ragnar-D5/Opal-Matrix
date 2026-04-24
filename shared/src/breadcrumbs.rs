use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Clone, Default, Deserialize)]
pub struct Breadcrumbs {
    #[serde(default)]
    pub recent_rooms: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_dm_room_id: Option<String>,

    #[serde(default)]
    pub last_space_ids: HashMap<String, String>,
}
