use super::AccountDataPayload;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Clone, Default, Deserialize)]
pub struct Breadcrumbs {
    #[serde(default)]
    pub recent_rooms: Vec<String>,

    #[serde(default)]
    pub last_space_ids: HashMap<String, String>,
}

impl super::AccountData for Breadcrumbs {
    const DATA_KEY: &'static str = "org.opal-matrix.breadcrumbs";
}
