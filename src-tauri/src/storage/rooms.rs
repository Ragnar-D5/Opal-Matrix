use super::DataBaseModel;
use crate::TauriError;

pub struct RoomRow {
    // pub room_id: String,
    // pub name: Option<String>,
    // pub topic: Option<String>,
    // pub avatar_url: Option<String>,

    // pub algorithm: Option<String>,

    // pub is_direct: bool,
    // pub join_rule: JoinRule,
    // pub history_visibility: HistoryVisibility,
    // pub guest_access: GuestAccess,

    // pub power_levels: Option<String>,

    // pub room_type: Option<String>,
    // pub prev_batch: Option<String>,

    // pub highlight_count: Option<u32>,
    // pub notification_count: Option<u32>,
}

impl DataBaseModel for RoomRow {
    fn create_table(conn: &rusqlite::Connection) -> Result<(), crate::TauriError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rooms (
                room_id TEXT PRIMARY KEY,
                name TEXT,
                topic TEXT,
                avatar_url TEXT,

                algorithm TEXT,

                is_direct BOOLEAN NOT NULL DEFAULT 0,
                join_rule TEXT NOT NULL CHECK(join_rule IN ('public', 'invite', 'knock', 'private', 'restricted', 'knock_restricted')) DEFAULT 'private',
                history_visibility TEXT NOT NULL CHECK(history_visibility IN ('world_readable', 'shared', 'invited', 'joined')) DEFAULT 'shared',
                guest_access TEXT NOT NULL CHECK(guest_access IN ('can_join', 'forbidden')) DEFAULT 'forbidden',

                power_levels TEXT,

                room_type TEXT,
                prev_batch TEXT,

                highlight_count INTEGER,
                notification_count INTEGER
            )",
        )?;
        Ok(())
    }
}

#[derive(Default, Debug, Clone)]
pub struct RoomUpdate {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub power_levels: Option<String>,
    pub guest_access: Option<String>,
    pub history_visibility: Option<String>,
    pub join_rule: Option<String>,
    pub algorithm: Option<String>,
    pub room_type: Option<String>,
    pub prev_batch: Option<String>,
    pub highlight_count: Option<u32>,
    pub notification_count: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct SpaceChildRow {
    pub parent_room_id: String,
    pub child_room_id: String,
    pub order_str: Option<String>,
    pub is_deleted: bool,
}

impl DataBaseModel for SpaceChildRow {
    fn create_table(conn: &rusqlite::Connection) -> Result<(), crate::TauriError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS space_children (
                parent_room_id TEXT NOT NULL,
                child_room_id TEXT NOT NULL,
                order_str TEXT,
                PRIMARY KEY (parent_room_id, child_room_id),
                FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id)
            )",
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SpaceParentRow {
    pub child_room_id: String,
    pub parent_room_id: String,
    pub is_canonical: bool,
    pub is_deleted: bool,
}

impl DataBaseModel for SpaceParentRow {
    fn create_table(conn: &rusqlite::Connection) -> Result<(), crate::TauriError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS space_parents (
                child_room_id TEXT NOT NULL,
                parent_room_id TEXT NOT NULL,
                is_canonical BOOLEAN NOT NULL DEFAULT 0,
                PRIMARY KEY (child_room_id, parent_room_id),
                FOREIGN KEY (child_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
            )",
        )?;
        Ok(())
    }
}

pub fn save_prev_token(
    conn: &rusqlite::Connection,
    room_id: &String,
    prev_batch: &String,
) -> Result<(), TauriError> {
    conn.execute(
        "UPDATE rooms SET prev_batch = ? WHERE room_id = ?",
        rusqlite::params![prev_batch, room_id],
    )?;
    Ok(())
}
