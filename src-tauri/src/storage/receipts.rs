use log::info;
use std::sync::Arc;

use tauri::{State, command};

use super::DataBaseModel;
use crate::{AppState, TauriError};

#[derive(Debug, Clone)]
pub struct ReadReceiptRow {
    pub room_id: String,
    pub user_id: String,
    pub receipt_type: String,
    pub event_id: String,
    pub ts: i64,
}

impl DataBaseModel for ReadReceiptRow {
    fn create_table(conn: &rusqlite::Connection) -> Result<(), TauriError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS read_receipts (
                room_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                receipt_type TEXT NOT NULL,
                event_id TEXT NOT NULL,
                ts INTEGER NOT NULL,
                PRIMARY KEY (room_id, user_id, receipt_type),
                FOREIGN KEY(room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
            )",
        )?;

        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_read_receipts_event_id ON read_receipts(event_id);",
        )?;

        Ok(())
    }
}

#[command(rename_all = "snake_case")]
pub async fn get_receipt(
    state: State<'_, Arc<AppState>>,
    room_id: String,
) -> Result<Option<String>, TauriError> {
    let user_id = {
        let client_guard = state.client.read().await;
        client_guard
            .as_ref()
            .ok_or("Not logged in")?
            .user_id
            .clone()
    };

    let conn_guard = state.connection.lock().await;
    let conn = conn_guard
        .as_ref()
        .ok_or("Database connection not available")?;

    let mut stmt = conn.prepare(
        "SELECT event_id FROM read_receipts
         WHERE room_id = ?1 AND user_id = ?2 LIMIT 1",
    )?;

    let mut rows = stmt.query(rusqlite::params![room_id, user_id])?;
    info!(
        "Querying read receipt for room_id: {}, user_id: {}",
        room_id, user_id
    );

    if let Some(row) = rows.next()? {
        let event_id: String = row.get(0)?;
        Ok(Some(event_id))
    } else {
        Ok(None)
    }
}
