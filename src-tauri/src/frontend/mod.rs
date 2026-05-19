use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use shared::{api::signals::MessagesUpdate, messages::{MessageKind, UiMessage}, user_profile::UserProfile};
use tauri::{AppHandle, Emitter};

use crate::{
    TauriError, send_notification,  storage::{fetch_sidebar, members::MemberRow, rooms::get_room_name}
};

pub(crate) mod messages;
pub(crate) mod sidebar;
pub(crate) mod commands;

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

/// Emits message to the frontend in the `messages_update` event.
pub fn emit_messages_update(
    handle: &AppHandle,
    payload: &MessagesUpdate,
) -> Result<(), TauriError> {
    handle.emit("messages_update", payload)?;
    Ok(())
}

pub fn emit_single_message_update(
    handle: &AppHandle,
    room_id: &String,
    message: &UiMessage,
) -> Result<(), TauriError> {
    let payload = MessagesUpdate {
       new_messages: HashMap::from([(room_id.clone(), vec![message.clone()])]),
       updated_messages: HashMap::new(),
       messages_to_remove: HashSet::new(),
    };
    emit_messages_update(handle, &payload)
}

/// Returns a list of event IDs for messages that have been altered in any way (edited, deleted, failed to send) and need to be re-fetched from the server to update the frontend.
// fn get_events_to_resend(messages: &Vec<UiMessage>) -> Vec<String> {
//     messages.iter().filter_map(|msg|
//         let message
//     );
// }

pub fn send_messages_update(
    handle: &AppHandle,
    conn: &Connection,
    user_id: &String,
    current_room_id: Option<String>,
    frontend_focused: bool,
    payload: MessagesUpdate,
    send_notifications: bool,
) -> Result<(), TauriError> {
    emit_messages_update(handle, &payload)?;

    if !send_notifications {
        return Ok(());
    }


    for (room_id, messages) in payload.new_messages {
        if messages.is_empty() || (current_room_id == Some(room_id.clone()) && frontend_focused) {
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
