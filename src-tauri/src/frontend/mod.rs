use std::collections::HashMap;

use log::{info};
use rusqlite::Connection;
use shared::{messages::{MessageKind, UiMessage}, user_profile::UserProfile};
use tauri::{AppHandle, Emitter};

use crate::{
    send_notification,
    storage::{fetch_sidebar, members::MemberRow, rooms::get_room_name},
    TauriError,
};

pub(crate) mod messages;
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

pub fn emit_messages_update(
    handle: &AppHandle,
    messages: &HashMap<String, Vec<UiMessage>>,
) -> Result<(), TauriError> {
    handle.emit("messages_update", messages)?;
    Ok(())
}

pub fn send_messages_update(
    handle: &AppHandle,
    conn: &Connection,
    user_id: &String,
    current_room_id: Option<String>,
    messages: HashMap<String, Vec<UiMessage>>,
    send_notifications: bool,
) -> Result<(), TauriError> {
    emit_messages_update(handle, &messages)?;

    info!("Checking for notifications to send...");
    if !send_notifications {
        return Ok(());
    }


    // TODO: Also check if the app is focused and only send notifications if it's not or the room_id of the message isn't currently open
    for (room_id, messages) in messages {
        if messages.is_empty() || current_room_id == Some(room_id.clone()) {
            continue;
        }
        let room_name = get_room_name(conn, user_id, &room_id)?.unwrap_or(room_id.clone());

        for message in messages {
            if message.sender_id != *user_id
                && let MessageKind::UserMessage(user_message) = &message.kind
            {
                let title = room_name.clone();
                let body = user_message.display_string();

                send_notification(handle, title, body)?;
            }
        }
    }

    Ok(())
}
