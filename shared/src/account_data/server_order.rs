use ruma::OwnedRoomId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Default, Deserialize)]
pub struct ServerOrder {
    #[serde(default)]
    pub servers: Vec<OwnedRoomId>,
}
