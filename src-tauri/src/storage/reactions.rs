use crate::storage::DataBaseModel;

#[derive(Clone, Debug)]
pub struct ReactionRow {
    pub event_id: String,
    pub room_id: String,
    pub target_event_id: String,
    pub sender_id: String,
    pub reaction_key: String,
    pub timestamp: u64,
}

impl DataBaseModel for ReactionRow {
    fn create_table(conn: &rusqlite::Connection) -> Result<(), crate::TauriError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS reactions (
                event_id TEXT PRIMARY KEY,
                room_id TEXT NOT NULL,
                target_event_id TEXT NOT NULL,
                sender_id TEXT NOT NULL,
                reaction_key TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                FOREIGN KEY (target_event_id) REFERENCES messages(event_id)
                FOREIGN KEY (room_id) REFERENCES rooms(room_id)
            )",
        )?;
        Ok(())
    }
}
