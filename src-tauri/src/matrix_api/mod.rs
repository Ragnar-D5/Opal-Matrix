use tauri::{
    Runtime,
    plugin::{Builder, TauriPlugin},
};

use crate::{
    AppState, TauriError,
    matrix_api::rooms::get_members_api,
    storage::{SafeStuff, SyncCallsToExecute},
};

pub(crate) mod account_data;
pub(crate) mod authentication;
pub(crate) mod crypto;
pub(crate) mod discovery;
pub(crate) mod login_flow;
pub(crate) mod rooms;
pub(crate) mod sync;

pub async fn handle_sync_calls(
    token: &String,
    matrix_url: &String,
    sync_calls: SyncCallsToExecute,
) -> Result<SafeStuff, TauriError> {
    let mut stuff = SafeStuff::default();

    for room_id in sync_calls.get_members {
        stuff
            .memberships
            .extend(get_members_api(token, matrix_url, room_id.to_string()).await?);
    }

    Ok(stuff)
}
