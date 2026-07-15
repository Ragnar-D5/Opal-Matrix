use std::collections::HashMap;

use ruma::OwnedRoomId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Default, Deserialize)]
pub struct Breadcrumbs {
    #[serde(default)]
    pub recent_rooms: Vec<OwnedRoomId>,
    #[serde(default)]
    pub last_space_ids: HashMap<OwnedRoomId, OwnedRoomId>,
    #[serde(default)]
    pub last_dm_id: Option<OwnedRoomId>,
    #[serde(default)]
    pub last_single_id: Option<OwnedRoomId>,
    #[serde(default)]
    pub dms_last: bool,
}
