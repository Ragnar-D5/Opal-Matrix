use super::DataBaseModel;

#[derive(Debug, Clone)]
pub struct MessageRow {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub msg_type: String,
    pub body: Option<String>,
    pub raw_json: String,
    pub timestamp: i64,
}

impl DataBaseModel for MessageRow {
    fn create_table(conn: &rusqlite::Connection) -> Result<(), crate::TauriError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                    event_id TEXT PRIMARY KEY,
                    room_id TEXT NOT NULL,
                    sender TEXT NOT NULL,
                    msg_type TEXT NOT NULL,
                    body TEXT,
                    raw_json TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    FOREIGN KEY (room_id) REFERENCES rooms(room_id)
                );

                CREATE INDEX IF NOT EXISTS idx_messages_room_id ON messages(room_id);
                CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
                CREATE INDEX IF NOT EXISTS idx_messages_room_timestamp ON messages(room_id, timestamp);
            ",
        )?;
        Ok(())
    }
}
