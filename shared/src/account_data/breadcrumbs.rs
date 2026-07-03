use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Default, Deserialize)]
pub struct Breadcrumbs {
    #[serde(default)]
    pub recent_rooms: Vec<String>,
    #[serde(default)]
    pub last_space_ids: HashMap<String, String>,
    #[serde(default)]
    pub dms_last: bool,
}
