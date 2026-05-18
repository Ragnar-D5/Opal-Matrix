use rusqlite::Connection;

use crate::{storage::DataBaseModel, TauriError};

#[derive(Debug, Clone)]
pub struct ReactionRow {
    pub event_id: String,
    pub room_id: String,
    pub target_event_id: String,
    pub sender_id: String,
    pub reaction: String,
    pub timestamp: u64,
}

impl DataBaseModel for ReactionRow {
    fn create_table(conn: &Connection) -> Result<(), TauriError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS reactions (
            event_id TEXT PRIMARY KEY,
            room_id TEXT NOT NULL,
            target_event_id TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            reaction TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            FOREIGN KEY (room_id) REFERENCES rooms(room_id),
            FOREIGN KEY (event_id) REFERENCES messages(event_id),
            FOREIGN KEY (target_event_id) REFERENCES messages(event_id)
        )",
        )?;
        Ok(())
    }
}

pub fn delete_reactions_where_event_id_deleted(conn: &Connection) -> Result<(), TauriError> {
    conn.execute(
        "DELETE FROM reactions WHERE target_event_id IN (SELECT event_id FROM messages WHERE state = 'deleted') OR event_id IN (SELECT event_id FROM messages WHERE state = 'deleted')",
        [],
    )?;
    Ok(())
}
