use rusqlite::Connection;

use crate::{storage::DataBaseModel, TauriError};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
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

pub fn save_reactions(
    conn: &mut Connection,
    reactions: Vec<ReactionRow>,
) -> Result<(), TauriError> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO reactions (event_id, room_id, target_event_id, sender_id, reaction, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for reaction in reactions {
            stmt.execute([
                &reaction.event_id,
                &reaction.room_id,
                &reaction.target_event_id,
                &reaction.sender_id,
                &reaction.reaction,
                &reaction.timestamp.to_string(),
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn delete_reactions_where_event_id_deleted(conn: &Connection) -> Result<(), TauriError> {
    conn.execute(
        "DELETE FROM reactions WHERE event_id IN (SELECT event_id FROM messages WHERE state = 'deleted')",
        [],
    )?;
    Ok(())
}

pub fn get_reactions_for_message(
    conn: &Connection,
    event_id: &String,
) -> Result<Vec<ReactionRow>, TauriError> {
    let mut stmt = conn.prepare(
        "SELECT event_id, room_id, target_event_id, sender_id, reaction, timestamp FROM reactions WHERE target_event_id = ?1",
    )?;
    let rows = stmt.query_map([event_id], |row| {
        Ok(ReactionRow {
            event_id: row.get(0)?,
            room_id: row.get(1)?,
            target_event_id: row.get(2)?,
            sender_id: row.get(3)?,
            reaction: row.get(4)?,
            timestamp: row.get(5)?,
        })
    })?;

    let mut reactions = Vec::new();
    for reaction in rows {
        reactions.push(reaction?);
    }
    Ok(reactions)
}

#[derive(Debug, Clone)]
pub struct MessageEditRow {
    pub event_id: String,
    pub target_event_id: String,
    pub new_json: String,
    pub timestamp: u64,
}

impl DataBaseModel for MessageEditRow {
    fn create_table(conn: &Connection) -> Result<(), TauriError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS message_edits (
            event_id TEXT PRIMARY KEY,
            target_event_id TEXT NOT NULL,
            new_json TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            FOREIGN KEY (target_event_id) REFERENCES messages(event_id)
        )",
        )?;
        Ok(())
    }
}

pub fn normalize_edits(conn: &mut Connection) -> Result<(), TauriError> {
    conn.execute(
        "UPDATE messages
    SET last_edited_id = (
        SELECT event_id
        FROM message_edits
        WHERE message_edits.target_event_id = messages.event_id
        ORDER BY timestamp DESC
        LIMIT 1
    )
    WHERE EXISTS (
        SELECT 1
        FROM message_edits
        WHERE message_edits.target_event_id = messages.event_id
    );",
        [],
    )?;
    Ok(())
}
