use serde::Serialize;

use crate::storage::messages::MessageRow;

#[derive(Debug, Serialize, Clone)]
pub struct Message {
    pub room_id: String,
    pub msg_type: String,
    pub id: String,
    pub ts: i64,
    pub raw_json: String,
    pub sender: String,
}

impl From<MessageRow> for Message {
    fn from(row: MessageRow) -> Self {
        Self {
            room_id: row.room_id,
            id: row.event_id,
            ts: row.timestamp,
            raw_json: row.raw_json,
            msg_type: row.msg_type,
            sender: row.sender,
        }
    }
}
