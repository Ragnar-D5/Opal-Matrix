use matrix_sdk::{
    config::SyncSettings, ruma::events::space::parent::SyncSpaceParentEvent,
    Client as MatrixClient, LoopCtrl, SessionChange,
};
use tauri::{async_runtime::spawn, AppHandle};

use crate::{
    frontend::sidebar::send_sidebar,
    matrix_api::crypto::{save_session, StoredSession},
    TauriError,
};

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
                    expires_at: None,
                    next_batch: None,
                    recovery_key: None,
                    homeserver_url: client_clone.homeserver().to_string(),
                };
                save_session(&kr_session).await.unwrap_or_else(|e| {
                    log::error!("Failed to save session after token refresh: {:?}", e);
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

    let client_sync_clone = client.clone();
    let handle_clone = handle.clone();
    spawn(async move {
        let sync_settings = SyncSettings::default()
            .ignore_timeout_on_first_sync(true)
            .timeout(std::time::Duration::from_secs(30));

        if let Err(e) = client_sync_clone.sync_once(sync_settings.clone()).await {
            log::error!("Initial sync failed: {e}");
        };

        send_sidebar(&client_sync_clone.joined_rooms(), &handle_clone)
            .await
            .unwrap_or_else(|e| {
                log::error!("Failed to send sidebar after initial sync: {:?}", e);
            });

        let sync_result = client_sync_clone
            .sync_with_result_callback(sync_settings, |ev| {
                let client_sync_clone = client_sync_clone.clone();
                let handle_clone = handle_clone.clone();
                async move {
                    let client_clone_dwa = client_sync_clone.clone();
                    let handle_clone = handle_clone.clone();

                    log::debug!("Received sync event: {:?}", ev);
                    send_sidebar(&client_clone_dwa.joined_rooms(), &handle_clone)
                        .await
                        .unwrap_or_else(|e| {
                            log::error!("Failed to send sidebar after initial sync: {:?}", e);
                        });
                    Ok(LoopCtrl::Continue)
                }
            })
            .await;

        if let Err(e) = sync_result {
            log::error!("Sync loop exited with error: {e}");
        }
    });

    log::info!("Sync loop started");
    Ok(())
}
