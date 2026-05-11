use crate::{
    TauriError,
    matrix_api::rooms::get_members_api,
    state::HomeServerInfo,
    storage::{SafeStuff, SyncCallsToExecute},
};

pub(crate) mod account_data;
pub(crate) mod authentication;
pub(crate) mod crypto;
pub(crate) mod discovery;
pub(crate) mod rooms;
pub(crate) mod sync;

pub async fn handle_sync_calls(
    server_info: HomeServerInfo,
    access_token: String,
    sync_calls: SyncCallsToExecute,
) -> Result<SafeStuff, TauriError> {
    let mut stuff = SafeStuff::default();

    for room_id in sync_calls.get_members {
        stuff.memberships.extend(
            get_members_api(&server_info, access_token.clone(), room_id.to_string()).await?,
        );
    }

    Ok(stuff)
}
