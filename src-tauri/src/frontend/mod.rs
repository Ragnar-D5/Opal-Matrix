use std::collections::HashMap;

use shared::user_profile::UserProfile;
use tauri::{AppHandle, Emitter};

use crate::TauriError;

pub(crate) mod commands;
pub(crate) mod messages;
pub(crate) mod sidebar;

pub fn send_member_update(
    handle: &AppHandle,
    payload: Vec<HashMap<String, UserProfile>>,
) -> Result<(), TauriError> {
    if payload.is_empty() {
        return Ok(());
    }

    handle.emit("member_update", payload)?;

    Ok(())
}
