use std::collections::HashMap;
use std::collections::HashSet;

use crate::TauriError;
use ruma::OwnedRoomId;
use rusqlite::params;
use rusqlite::Connection;

pub(crate) mod members;
pub(crate) mod messages;
pub(crate) mod rooms;

use members::MemberRow;
use messages::MessageRow;
use rooms::{RoomRow, RoomUpdate, SpaceChildRow, SpaceParentRow};

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
    SpaceChildRow::create_table(&conn)?;
    SpaceParentRow::create_table(&conn)?;

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

    pub direct_rooms: Option<HashSet<OwnedRoomId>>,

    pub room_updates: HashMap<OwnedRoomId, RoomUpdate>,

    pub space_children: Vec<SpaceChildRow>,

    pub space_parents: Vec<SpaceParentRow>,
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

    let mut stmt_space_children = tx.prepare(
        "INSERT INTO space_children (parent_room_id, child_room_id, order_str)
        VALUES (?, ?, ?)
        ON CONFLICT(parent_room_id, child_room_id) DO UPDATE SET
            order_str = excluded.order_str",
    )?;
    let mut stmt_delete_space_child =
        tx.prepare("DELETE FROM space_children WHERE parent_room_id = ? AND child_room_id = ?")?;

    let mut stmt_space_parents = tx.prepare(
        "INSERT INTO space_parents (child_room_id, parent_room_id, is_canonical)
        VALUES (?, ?, ?)
        ON CONFLICT(child_room_id, parent_room_id) DO UPDATE SET
            is_canonical = excluded.is_canonical",
    )?;
    let mut stmt_delete_space_parent =
        tx.prepare("DELETE FROM space_parents WHERE child_room_id = ? AND parent_room_id = ?")?;

    for child_update in changes.space_children {
        if child_update.is_deleted {
            stmt_delete_space_child.execute(params![
                child_update.parent_room_id,
                child_update.child_room_id
            ])?;
        } else {
            stmt_space_children.execute(params![
                child_update.parent_room_id,
                child_update.child_room_id,
                child_update.order_str
            ])?;
        }
    }

    for parent_update in changes.space_parents {
        if parent_update.is_deleted {
            stmt_delete_space_parent.execute(params![
                parent_update.child_room_id,
                parent_update.parent_room_id
            ])?;
        } else {
            stmt_space_parents.execute(params![
                parent_update.child_room_id,
                parent_update.parent_room_id,
                parent_update.is_canonical
            ])?;
        }
    }

    drop(stmt_space_children);
    drop(stmt_delete_space_child);
    drop(stmt_space_parents);
    drop(stmt_delete_space_parent);

    if let Some(rooms) = changes.direct_rooms {
        tx.execute("UPDATE rooms SET is_direct = 0", [])?;

        let mut stmt_update_direct =
            tx.prepare("UPDATE rooms SET is_direct = 1 WHERE room_id = ?")?;
        for room_id in rooms {
            stmt_update_direct.execute(params![room_id.to_string()])?;
        }
        drop(stmt_update_direct);
    }

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
        algorithm = COALESCE(?, algorithm),
        room_type = COALESCE(?, room_type)
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
            update.room_type,
            room_id.to_string()
        ])?;
    }
    drop(stmt_room_state);

    tx.commit()?;

    Ok(())
}
