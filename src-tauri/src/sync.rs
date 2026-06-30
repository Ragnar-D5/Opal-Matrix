use matrix_sdk::{
    config::SyncSettings, ruma::presence::PresenceState, Client as MatrixClient, SessionChange,
};
use shared::synth::ProfileAudio;
use std::pin::pin;
use std::sync::{Arc, Mutex};
use tauri::{async_runtime::spawn, AppHandle};

use crate::frontend::profiles::handle_typing_notice;
use crate::matrix_api::matrixrtc::handle_call_member_change;
use crate::send_event;
use crate::settings::handle_account_data_event;
use crate::{
    frontend::{
        presence::handle_presences,
        profiles::{on_member_update, send_all_members},
        sidebar::{extract_call_memberships, handle_room_updates, send_sidebar},
    },
    matrix_api::{
        keyring::{save_session, StoredSession},
        matrixrtc::{cleanup_ghost_calls, handle_to_device_messages},
        profile::{client_user_profile_event_handle, send_user_to_frontend, ProfileDebounce},
    },
    TauriError,
};
use futures_util::StreamExt;

pub async fn attach_callbacks(
    client: &MatrixClient,
    handle: &AppHandle,
    default_audio: &ProfileAudio,
) -> Result<(), TauriError> {
    let mut session_subscriber = client.subscribe_to_session_changes();
    let client_clone = client.clone();

    let Some(own_id) = client.user_id().map(|v| v.to_string()) else {
        return Err("Failed to get own user ID from client".into());
    };

    cleanup_ghost_calls(client).await;

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

    let rooms = client.rooms();

    send_sidebar(&rooms, handle, &own_id).await?;
    send_user_to_frontend(handle, client, default_audio).await?;

    let members_client = client.clone();
    let members_handle = handle.clone();
    let members_rooms = rooms.clone();
    let audio_clone = default_audio.clone();
    spawn(async move {
        if let Err(e) = send_all_members(
            &members_client,
            &members_handle,
            &members_rooms,
            &audio_clone,
        )
        .await
        {
            log::error!("Failed to send all members: {:?}", e);
        }
    });

    if let Some(data) = extract_call_memberships(&rooms).await {
        send_event(handle, &data);
    }

    let client_sync_clone = client.clone();
    let handle_clone = handle.clone();
    tauri::async_runtime::spawn(async move {
        let sync_settings = SyncSettings::default()
            .set_presence(PresenceState::Online)
            .timeout(std::time::Duration::from_secs(30));

        let sync_stream = client_sync_clone.sync_stream(sync_settings).await;

        let mut sync_stream = pin!(sync_stream);

        log::info!("Started background sync stream");

        while let Some(sync_item) = sync_stream.next().await {
            match sync_item {
                Ok(sync_result) => {
                    log::debug!("Received sync");
                    if let Err(e) =
                        handle_to_device_messages(sync_result.to_device, handle_clone.clone()).await
                    {
                        log::error!("Failed to handle to-device messages: {:?}", e);
                    };
                    handle_presences(&sync_result.presence, &handle_clone);
                    handle_room_updates(
                        &sync_result.rooms,
                        &client_sync_clone,
                        &handle_clone,
                        &own_id,
                    )
                    .await;
                }
                Err(e) => {
                    log::error!("Sync loop exited with error: {}", e);
                }
            }
        }
    });

    client.add_event_handler_context(handle.clone());
    client.add_event_handler_context(default_audio.clone());

    let debounce = Arc::new(Mutex::new(ProfileDebounce::default()));
    client.add_event_handler_context(debounce);

    client.add_event_handler(on_member_update);
    client.add_event_handler(client_user_profile_event_handle);
    client.add_event_handler(handle_call_member_change);
    client.add_event_handler(handle_typing_notice);
    client.add_event_handler(handle_account_data_event);

    Ok(())
}
