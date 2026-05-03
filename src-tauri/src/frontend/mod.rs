use std::collections::HashMap;

use rusqlite::Connection;
use shared::{messages::UiMessage, user_profile::UserProfile};
use tauri::{AppHandle, Emitter};

use crate::{
    TauriError,
    storage::{fetch_sidebar, members::MemberRow},
};

pub(crate) mod sidebar;

pub fn send_member_update(handle: &AppHandle, updates: Vec<MemberRow>) -> Result<(), TauriError> {
    if updates.is_empty() {
        return Ok(());
    }

    let payload: HashMap<String, HashMap<String, UserProfile>> =
        updates.into_iter().fold(HashMap::new(), |mut acc, row| {
            let user_profile = UserProfile {
                user_id: row.user_id.clone(),
                display_name: row.display_name,
                avatar_url: row.avatar_url,
            };

            acc.entry(row.room_id)
                .or_insert_with(HashMap::new)
                .insert(row.user_id, user_profile);

            acc
        });

    handle.emit("member_update", payload)?;

    Ok(())
}

pub fn send_sidebar_update(
    conn: &Connection,
    handle: &AppHandle,
    own_user_id: &String,
) -> Result<(), TauriError> {
    let (all_rooms, parent_to_children, all_children) = fetch_sidebar(conn, own_user_id)?;

    let tree = sidebar::build_tree(
        conn,
        own_user_id,
        all_rooms,
        parent_to_children,
        all_children,
    );

    handle.emit("sidebar_update", tree)?;

    Ok(())
}

pub fn send_messages_update(
    handle: &AppHandle,
    messages: HashMap<String, Vec<UiMessage>>,
) -> Result<(), TauriError> {
    handle.emit("messages_update", messages)?;

    Ok(())
}
