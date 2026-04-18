use serde::Serialize;

use crate::storage::messages::MessageRow;

#[derive(Debug, Serialize, Clone)]
pub struct Message {
    pub msg_type: String,
    pub id: String,
    pub content: Option<String>,
    pub ts: i64,
    pub raw_json: String,
    pub sender: String,
}

impl From<MessageRow> for Message {
    fn from(row: MessageRow) -> Self {
        Self {
            id: row.event_id,
            content: row.body,
            ts: row.timestamp,
            raw_json: row.raw_json,
            msg_type: row.msg_type,
            sender: row.sender,
        }
    }
}
