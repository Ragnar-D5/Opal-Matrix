use std::collections::HashMap;

use crate::storage::rooms::RoomUpdate;
use crate::TauriError;
use ruma::OwnedRoomId;
use rusqlite::params;
use rusqlite::Connection;

pub(crate) mod members;
pub(crate) mod messages;
pub(crate) mod rooms;

use members::MemberRow;
use messages::MessageRow;
use rooms::RoomRow;

pub async fn init_storage(
    path: std::path::PathBuf,
    device_id: &String,
    db_passphrase: &String,
) -> Result<(bool, Connection), TauriError> {
    let db_path = path.join(format!("{}.db", device_id));

    let db_exists = db_path.exists();

    let conn = Connection::open(&db_path).map_err(|e| format!("Failed to open database: {e}"))?;

    conn.pragma_update(None, "key", db_passphrase)?;

    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
        .map_err(|e| format!("Failed to access database: {e}"))?;

    RoomRow::create_table(&conn)?;
    MessageRow::create_table(&conn)?;
    MemberRow::create_table(&conn)?;

    Ok((db_exists, conn))
}

pub trait DataBaseModel {
    fn create_table(conn: &Connection) -> Result<(), TauriError>;
}

#[derive(Default, Debug)]
pub struct SyncChanges {
    pub joined_rooms: Vec<OwnedRoomId>,
    pub new_messages: Vec<MessageRow>,
    pub member_updates: Vec<MemberRow>,

    pub room_updates: HashMap<OwnedRoomId, RoomUpdate>,
}

pub async fn apply_sync_changes(
    conn: &mut Connection,
    changes: SyncChanges,
) -> Result<(), TauriError> {
    let tx = conn.transaction()?;

    let mut stmt_ensure_rooms = tx.prepare("INSERT OR IGNORE INTO rooms (room_id) VALUES (?)")?;

    for room_id in &changes.joined_rooms {
        stmt_ensure_rooms.execute(params![room_id.to_string()])?;
    }
    drop(stmt_ensure_rooms);

    let mut stmt_messages = tx.prepare(
        "INSERT INTO messages (event_id, room_id, sender, msg_type, body, raw_json, timestamp)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(event_id) DO UPDATE SET
            body = excluded.body,
            msg_type = excluded.msg_type,
            raw_json = excluded.raw_json",
    )?;
    for msg in changes.new_messages {
        stmt_messages.execute(params![
            msg.event_id,
            msg.room_id,
            msg.sender,
            msg.msg_type,
            msg.body,
            msg.raw_json,
            msg.timestamp
        ])?;
    }
    drop(stmt_messages);

    let mut stmt_members = tx.prepare(
        "INSERT INTO members (room_id, user_id, display_name, avatar_url, membership)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(room_id, user_id) DO UPDATE SET
            display_name = excluded.display_name,
            avatar_url = excluded.avatar_url,
            membership = excluded.membership",
    )?;
    for member in changes.member_updates {
        stmt_members.execute(params![
            member.room_id,
            member.user_id,
            member.display_name,
            member.avatar_url,
            member.membership
        ])?;
    }
    drop(stmt_members);

    let mut stmt_room_state = tx.prepare(
        "UPDATE rooms SET
        name = COALESCE(?, name),
        topic = COALESCE(?, topic),
        avatar_url = COALESCE(?, avatar_url),
        power_levels = COALESCE(?, power_levels),
        guest_access = COALESCE(?, guest_access),
        history_visibility = COALESCE(?, history_visibility),
        join_rule = COALESCE(?, join_rule),
        algorithm = COALESCE(?, algorithm)
    WHERE room_id = ?",
    )?;
    for (room_id, update) in changes.room_updates {
        stmt_room_state.execute(params![
            update.name,
            update.topic,
            update.avatar_url,
            update.power_levels,
            update.guest_access,
            update.history_visibility,
            update.join_rule,
            update.algorithm,
            room_id.to_string()
        ])?;
    }
    drop(stmt_room_state);

    tx.commit()?;

    Ok(())
}
