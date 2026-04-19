use rusqlite::Connection;

use crate::{frontend::messages::Message, TauriError};

use super::DataBaseModel;

#[derive(Debug, Clone)]
pub struct MessageRow {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub msg_type: String,
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
                    raw_json TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    FOREIGN KEY (room_id) REFERENCES rooms(room_id)
                );
            ",
        )?;
        Ok(())
    }
}

pub fn get_messages(
    conn: &Connection,
    room_id: &String,
    oldest_id: Option<String>,
    limit: usize,
) -> Result<Vec<MessageRow>, TauriError> {
    let mut messages = Vec::new();

    match oldest_id {
        Some(id) => {
            let mut stmt = conn.prepare(
                "SELECT event_id, room_id, sender, msg_type, raw_json, timestamp
            FROM MESSAGES
            WHERE room_id = ?
                AND timestamp < (SELECT timestamp FROM MESSAGES WHERE event_id = ?)
                AND event_id != ?
            ORDER BY timestamp DESC
            LIMIT ?",
            )?;

            let rows = stmt.query_map(rusqlite::params![room_id, id, id, limit], |row| {
                Ok(MessageRow {
                    event_id: row.get(0)?,
                    room_id: row.get(1)?,
                    sender: row.get(2)?,
                    msg_type: row.get(3)?,
                    raw_json: row.get(4)?,
                    timestamp: row.get(5)?,
                })
            })?;

            for msg_res in rows {
                messages.push(msg_res?.into());
            }
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT event_id, room_id, sender, msg_type, raw_json, timestamp
            FROM MESSAGES
            WHERE room_id = ?
            ORDER BY timestamp DESC
            LIMIT ?",
            )?;

            let rows = stmt.query_map(rusqlite::params![room_id, limit], |row| {
                Ok(MessageRow {
                    event_id: row.get(0)?,
                    room_id: row.get(1)?,
                    sender: row.get(2)?,
                    msg_type: row.get(3)?,
                    raw_json: row.get(4)?,
                    timestamp: row.get(5)?,
                })
            })?;

            for msg_res in rows {
                messages.push(msg_res?);
            }
        }
    }

    Ok(messages)
}

pub fn save_messages(conn: &mut Connection, messages: Vec<MessageRow>) -> Result<(), TauriError> {
    let tx = conn.transaction()?;

    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO messages (event_id, room_id, sender, msg_type, raw_json, timestamp)
            VALUES (?, ?, ?, ?, ?, ?)",
        )?;

        for msg in messages {
            stmt.execute(rusqlite::params![
                msg.event_id,
                msg.room_id,
                msg.sender,
                msg.msg_type,
                msg.raw_json,
                msg.timestamp
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}
