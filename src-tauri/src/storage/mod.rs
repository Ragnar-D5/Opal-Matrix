use shared::user_profile::UserProfile;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use tauri::{command, State};

use crate::AppState;
use crate::TauriError;
use members::MemberRow;
use messages::MessageRow;
use receipts::ReadReceiptRow;
use rooms::{RoomRow, SpaceChildRow, SpaceParentRow};
use ruma::OwnedRoomId;
use rusqlite::params;
use rusqlite::Connection;
use shared::sidebar::FlatRoom;

pub(crate) mod members;
pub(crate) mod messages;
pub(crate) mod receipts;
pub(crate) mod rooms;

pub async fn init_storage(
    path: std::path::PathBuf,
    device_id: &String,
    db_passphrase: &String,
) -> Result<(bool, Connection), TauriError> {
    let db_path = path.join(format!("{}.db", device_id));

    let db_exists = db_path.exists();

    let conn = Connection::open(&db_path).map_err(|e| format!("Failed to open database: {e}"))?;

    if false {
        conn.pragma_update(None, "key", db_passphrase)?;

        conn.query_row("SELECT count(*) FROM sqlite_master", [], |_| Ok(()))
            .map_err(|e| format!("Failed to access database: {e}"))?;
    }

    RoomRow::create_table(&conn)?;
    MessageRow::create_table(&conn)?;
    MemberRow::create_table(&conn)?;
    SpaceChildRow::create_table(&conn)?;
    SpaceParentRow::create_table(&conn)?;
    ReadReceiptRow::create_table(&conn)?;

    Ok((db_exists, conn))
}

pub trait DataBaseModel {
    fn create_table(conn: &Connection) -> Result<(), TauriError>;
}

#[derive(Default, Debug, Clone)]
pub struct SyncChanges {
    pub joined_rooms: Vec<OwnedRoomId>,
    pub new_messages: Vec<MessageRow>,
    pub member_updates: Vec<MemberRow>,
    pub read_receipts: Vec<ReadReceiptRow>,

    pub direct_rooms: Option<HashSet<OwnedRoomId>>,

    pub room_updates: HashMap<OwnedRoomId, RoomRow>,

    pub space_children: Vec<SpaceChildRow>,

    pub space_parents: Vec<SpaceParentRow>,
}

pub struct SyncCallsToExecute {
    pub get_members: Vec<OwnedRoomId>,
}

#[derive(Default)]
pub struct SafeStuff {
    pub memberships: Vec<MemberRow>,
}

pub async fn apply_sync_changes(
    conn: &mut Connection,
    changes: SyncChanges,
) -> Result<SyncCallsToExecute, TauriError> {
    let mut new_rooms = Vec::new();

    let tx = conn.transaction()?;

    let mut stmt_room_exists = tx.prepare("SELECT room_id FROM rooms WHERE room_id = ?")?;
    let mut stmt_ensure_rooms = tx.prepare("INSERT OR IGNORE INTO rooms (room_id) VALUES (?)")?;

    for room_id in &changes.joined_rooms {
        let exists: bool = stmt_room_exists
            .query_row(params![room_id.to_string()], |_| Ok(()))
            .is_ok();

        if !exists {
            new_rooms.push(room_id.clone());
        }

        stmt_ensure_rooms.execute(params![room_id.to_string()])?;
    }
    drop(stmt_room_exists);
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
        "INSERT INTO messages (event_id, room_id, sender, msg_type, raw_json, timestamp)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(event_id) DO UPDATE SET
            msg_type = excluded.msg_type,
            raw_json = excluded.raw_json",
    )?;
    for msg in changes.new_messages {
        stmt_messages.execute(params![
            msg.event_id,
            msg.room_id,
            msg.sender,
            msg.msg_type,
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
        room_type = COALESCE(?, room_type),
        prev_batch = COALESCE(prev_batch, ?),
        highlight_count = COALESCE(?, highlight_count),
        notification_count = COALESCE(?, notification_count)
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
            update.prev_batch,
            update.highlight_count,
            update.notification_count,
            room_id.to_string()
        ])?;
    }
    drop(stmt_room_state);

    let mut stmt_receipts = tx.prepare(
        "INSERT INTO read_receipts (room_id, user_id, receipt_type, event_id, ts)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(room_id, user_id, receipt_type) DO UPDATE SET
            event_id = excluded.event_id,
            ts = excluded.ts",
    )?;
    for receipt in changes.read_receipts {
        stmt_receipts.execute(params![
            receipt.room_id,
            receipt.user_id,
            receipt.receipt_type,
            receipt.event_id,
            receipt.ts
        ])?;
    }
    drop(stmt_receipts);

    tx.commit()?;

    Ok(SyncCallsToExecute {
        get_members: new_rooms,
    })
}

pub fn fetch_sidebar(
    conn: &Connection,
    own_user_id: &String,
) -> Result<
    (
        HashMap<String, FlatRoom>,
        HashMap<String, Vec<String>>,
        HashSet<String>,
    ),
    TauriError,
> {
    let mut stmt = conn.prepare(
        "SELECT
            r.room_id,
            COALESCE(
                r.name,
                CASE
                    WHEN r.is_direct = 1 THEN (
                        SELECT COALESCE(m.display_name, m.user_id)
                        FROM members m
                        WHERE m.room_id = r.room_id
                          AND m.user_id != ?
                          AND m.membership IN ('join', 'invite')
                        ORDER BY
                            CASE WHEN m.membership = 'join' THEN 0 ELSE 1 END,
                            m.display_name IS NULL,
                            m.user_id
                        LIMIT 1
                    )
                    ELSE NULL
                END
            ) AS name,
            COALESCE(
                r.topic,
                CASE
                    WHEN r.is_direct = 1 THEN (
                        SELECT m.user_id
                        FROM members m
                        WHERE m.room_id = r.room_id
                          AND m.user_id != ?
                          AND m.membership IN ('join', 'invite')
                        ORDER BY
                            CASE WHEN m.membership = 'join' THEN 0 ELSE 1 END,
                            m.user_id
                        LIMIT 1
                    )
                    ELSE NULL
                END
            ) AS topic,
            COALESCE(
                r.avatar_url,
                CASE
                    WHEN r.is_direct = 1 THEN (
                        SELECT m.avatar_url
                        FROM members m
                        WHERE m.room_id = r.room_id
                            AND m.user_id != ?
                            AND m.membership IN ('join', 'invite')
                        ORDER BY
                            CASE WHEN m.membership = 'join' THEN 0 ELSE 1 END,
                            m.avatar_url IS NULL,
                            m.user_id
                        LIMIT 1
                    )
                    ELSE NULL
                END
            ) AS avatar_url,
            r.room_type,
            r.is_direct,
            (
                SELECT msg.timestamp
                FROM messages msg
                WHERE msg.room_id = r.room_id
                ORDER BY msg.timestamp DESC
                LIMIT 1
            ) AS last_ts,
            r.highlight_count,
            r.notification_count
        FROM rooms r",
    )?;

    let mut all_rooms: HashMap<String, FlatRoom> = HashMap::new();

    let room_iter = stmt.query_map([own_user_id, own_user_id, own_user_id], |row| {
        Ok(FlatRoom {
            room_id: row.get(0)?,
            name: row.get(1)?,
            topic: row.get(2)?,
            avatar_url: row.get(3)?,
            room_type: row.get(4)?,
            is_direct: row.get(5)?,
            last_ts: row.get(6)?,
            highlight_count: row.get(7)?,
            notification_count: row.get(8)?,
        })
    })?;

    for room in room_iter {
        let room = room?;

        all_rooms.insert(room.room_id.clone(), room);
    }

    // Only respect child links if the child isn't a space, or if it is a space AND has a valid backlink.
    // This prevents un-consented spaces from swallowing top-level servers.
    let mut stmt_links = conn.prepare(
        "SELECT sc.parent_room_id, sc.child_room_id
        FROM space_children sc
        LEFT JOIN rooms child_room ON sc.child_room_id = child_room.room_id
        LEFT JOIN space_parents sp ON sc.child_room_id = sp.child_room_id AND sc.parent_room_id = sp.parent_room_id
        WHERE child_room.room_id IS NULL
           OR child_room.room_type IS NULL
           OR child_room.room_type != 'm.space'
           OR sp.parent_room_id IS NOT NULL
        ORDER BY sc.order_str"
    )?;

    let mut parent_to_children: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_children: HashSet<String> = HashSet::new();

    let link_iter = stmt_links.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    for link in link_iter {
        if let Ok((parent_id, child_id)) = link {
            parent_to_children
                .entry(parent_id)
                .or_default()
                .push(child_id.clone());
            all_children.insert(child_id);
        }
    }

    Ok((all_rooms, parent_to_children, all_children))
}

#[command(rename_all = "snake_case")]
pub async fn get_members(
    state: State<'_, Arc<AppState>>,
    room_id: String,
) -> Result<HashMap<String, UserProfile>, TauriError> {
    let conn_guard = state.connection.lock().await;
    let conn = conn_guard
        .as_ref()
        .ok_or("Database connection not initialized")?;

    let mut stmt = conn.prepare(
        "SELECT room_id, user_id, display_name, avatar_url, membership
        FROM members
        WHERE room_id = ?
        UNION ALL
        SELECT ?, ?, 'room', NULL, 'join'",
    )?;

    let member_iter = stmt.query_map(
        params![room_id.as_str(), room_id.as_str(), room_id.as_str()],
        |row| {
            Ok(MemberRow {
                room_id: row.get(0)?,
                user_id: row.get(1)?,
                display_name: row.get(2)?,
                avatar_url: row.get(3)?,
                membership: row.get(4)?,
            })
        },
    )?;

    let mut members = HashMap::new();
    for member in member_iter {
        let member = member?;
        members.insert(
            member.user_id.clone(),
            UserProfile {
                user_id: member.user_id,
                display_name: member.display_name,
                avatar_url: member.avatar_url,
            },
        );
    }

    Ok(members)
}

pub async fn handle_safe_stuff(conn: &mut Connection, stuff: SafeStuff) -> Result<(), TauriError> {
    let tx = conn.transaction()?;

    let mut stmt_members = tx.prepare(
        "INSERT INTO members (room_id, user_id, display_name, avatar_url, membership)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(room_id, user_id) DO UPDATE SET
            display_name = excluded.display_name,
            avatar_url = excluded.avatar_url,
            membership = excluded.membership",
    )?;

    for member in stuff.memberships {
        stmt_members.execute(params![
            member.room_id,
            member.user_id,
            member.display_name,
            member.avatar_url,
            member.membership
        ])?;
    }

    Ok(())
}
