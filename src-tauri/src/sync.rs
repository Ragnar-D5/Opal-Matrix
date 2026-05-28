use matrix_sdk::{
    Client as MatrixClient, SessionChange,
    config::SyncSettings,
    ruma::{events::space::parent::SyncSpaceParentEvent, presence::PresenceState},
};
use std::pin::pin;
use tauri::{AppHandle, async_runtime::spawn};

use crate::{
    TauriError,
    frontend::{
        members::on_member_update,
        presence::handle_presences,
        sidebar::{handle_room_updates, send_sidebar},
    },
    matrix_api::keyring::{StoredSession, save_session},
};
use futures_util::StreamExt;

pub async fn attach_callbacks(client: &MatrixClient, handle: &AppHandle) -> Result<(), TauriError> {
    let mut session_subscriber = client.subscribe_to_session_changes();
    let client_clone = client.clone();

    spawn(async move {
        while let Ok(change) = session_subscriber.recv().await {
            if let SessionChange::TokensRefreshed = change {
                let Some(session) = client_clone.session() else {
                    log::error!("Session is None after token refresh");
                    continue;
                };

                let kr_session = StoredSession {
                    user_id: session.meta().user_id.to_string(),
                    device_id: session.meta().device_id.to_string(),
                    access_token: session.access_token().to_string(),
                    refresh_token: session.get_refresh_token().map(|t| t.to_string()),
                    homeserver_url: client_clone.homeserver().to_string(),
                };

                tokio::task::spawn_blocking(move || {
                    save_session(&kr_session).unwrap_or_else(|e| {
                        log::error!("Failed to save session after token refresh: {:?}", e);
                    })
                });
            }
        }
    });

    let client_sync_clone = client.clone();
    let handle_clone = handle.clone();
    client.add_event_handler(async move |_: SyncSpaceParentEvent| {
        send_sidebar(&client_sync_clone.joined_rooms(), &handle_clone)
            .await
            .unwrap_or_else(|e| {
                log::error!("Failed to send sidebar after space parent event: {:?}", e);
            });
    });

    send_sidebar(&client.joined_rooms(), handle).await?;

    let client_sync_clone = client.clone();
    let handle_clone = handle.clone();
    tauri::async_runtime::spawn(async move {
        let sync_settings = SyncSettings::default()
            .set_presence(PresenceState::Online)
            .ignore_timeout_on_first_sync(true)
            .timeout(std::time::Duration::from_secs(30));

        // 1. Get the stream (Note: depending on your matrix-sdk version,
        // you might not need `.await` on this specific line)
        let sync_stream = client_sync_clone.sync_stream(sync_settings).await;

        // 2. PIN THE STREAM!
        let mut sync_stream = pin!(sync_stream);

        log::info!("Started background sync stream");

        // 3. Now .next().await will work perfectly
        while let Some(sync_item) = sync_stream.next().await {
            match sync_item {
                Ok(sync_result) => {
                    log::debug!("Received sync event");

                    handle_presences(&sync_result.presence, &handle_clone);
                    handle_room_updates(&sync_result.rooms, &client_sync_clone, &handle_clone)
                        .await;
                }
                Err(e) => {
                    log::error!("Sync loop exited with error: {}", e);
                }
            }
        }
    });

    client.add_event_handler_context(handle.clone());
    client.add_event_handler(on_member_update);

    Ok(())
}
