use matrix_sdk::RoomState;
use matrix_sdk::ruma::OwnedRoomId;
use matrix_sdk::ruma::events::direct::DirectEventContent;
use matrix_sdk::{
    Client as MatrixClient, SessionChange, config::SyncSettings, ruma::presence::PresenceState,
};
use shared::sidebar::{DmList, RoomMapUpdate, RoomNode, ServerList, SingleList};
use shared::synth::ProfileAudio;
use std::collections::{HashMap, HashSet};
use std::pin::pin;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, async_runtime::spawn};

use crate::frontend::profiles::handle_typing_notice;
use crate::frontend::sidebar::{
    compute_dm_order, compute_single_order, convert_room_to_node, get_unknown_children,
    handle_account_data, spawn_all_children_update,
};
use crate::matrix_api::matrixrtc::handle_call_member_change;
use crate::send_event;
use crate::settings::handle_account_data_event;
use crate::{
    TauriError,
    frontend::{
        presence::handle_presences,
        profiles::{on_member_update, send_all_members},
        sidebar::{extract_call_memberships, handle_room_updates},
    },
    matrix_api::{
        keyring::{StoredSession, save_session},
        matrixrtc::{cleanup_ghost_calls, handle_to_device_messages},
        profile::{ProfileDebounce, client_user_profile_event_handle, send_user_to_frontend},
    },
};
use futures_util::StreamExt;

pub async fn attach_callbacks(
    client: &MatrixClient,
    handle: &AppHandle,
    default_audio: &ProfileAudio,
) -> Result<(), TauriError> {
    let mut session_subscriber = client.subscribe_to_session_changes();
    let client_clone = client.clone();

    let cleanup_client = client.clone();
    spawn(async move {
        cleanup_ghost_calls(&cleanup_client).await;
    });

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
    spawn(async move {
        let sync_settings = SyncSettings::default()
            .set_presence(PresenceState::Online)
            .timeout(std::time::Duration::from_secs(30));

        let sync_stream = client_sync_clone.sync_stream(sync_settings).await;

        let mut sync_stream = pin!(sync_stream);

        log::info!("Started background sync stream");

        let mut dm_map = client_sync_clone
            .account()
            .fetch_account_data_static::<DirectEventContent>()
            .await
            .map_err(|e| log::error!("Failed to fetch direct message account data: {:?}", e))
            .ok()
            .flatten()
            .and_then(|r| r.deserialize().ok());

        let mut prev_dm_ids = compute_dm_order(&client_sync_clone, &dm_map);

        let mut known_room_map: HashMap<OwnedRoomId, RoomNode> = futures_util::stream::iter(
            client_sync_clone
                .rooms()
                .into_iter()
                .filter(|room| !matches!(room.state(), RoomState::Banned | RoomState::Left)),
        )
        .map(|room| async move {
            let node = convert_room_to_node(&room).await?;
            Some((room.room_id().to_owned(), node))
        })
        .buffer_unordered(16)
        .filter_map(|entry| async move { entry })
        .collect()
        .await;

        log::info!("Initial room map");

        send_event(
            &handle_clone,
            &vec![RoomMapUpdate::Set {
                map: known_room_map
                    .clone()
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            }],
        );
        send_event(&handle_clone, &DmList(prev_dm_ids.clone()));

        let mut prev_single_ids = compute_single_order(&client_sync_clone, &known_room_map);
        send_event(&handle_clone, &SingleList(prev_single_ids.clone()));

        let mut prev_seen_servers: HashSet<OwnedRoomId> = known_room_map
            .iter()
            .filter_map(|(room_id, node)| {
                matches!(node, RoomNode::Server(_)).then_some(room_id.clone())
            })
            .collect();
        let servers: Vec<String> = prev_seen_servers.iter().map(|id| id.to_string()).collect();

        send_event(&handle_clone, &ServerList(servers));

        for room_id in &prev_seen_servers {
            spawn_all_children_update(
                client_sync_clone.clone(),
                handle_clone.clone(),
                room_id.clone(),
            );
        }

        let mut updates = Vec::new();
        for room_id in &prev_seen_servers {
            let Some(room) = client_sync_clone.get_room(room_id) else {
                continue;
            };
            updates
                .extend(get_unknown_children(&room, &client_sync_clone, &mut known_room_map).await);
        }

        if !updates.is_empty() {
            send_event(&handle_clone, &updates);
        }

        while let Some(sync_item) = sync_stream.next().await {
            match sync_item {
                Ok(sync_result) => {
                    log::trace!("Received sync");
                    if let Err(e) =
                        handle_to_device_messages(sync_result.to_device, handle_clone.clone()).await
                    {
                        log::error!("Failed to handle to-device messages: {:?}", e);
                    };

                    if let Some(new_dms) = handle_account_data(
                        &client_sync_clone,
                        &sync_result.account_data,
                        &mut dm_map,
                        &mut prev_dm_ids,
                    ) {
                        send_event(&handle_clone, &DmList(new_dms));
                    };

                    handle_presences(&sync_result.presence, &handle_clone);
                    handle_room_updates(
                        &sync_result.rooms,
                        &client_sync_clone,
                        &handle_clone,
                        &mut known_room_map,
                        &mut prev_seen_servers,
                    )
                    .await;

                    let dm_touched = sync_result
                        .rooms
                        .joined
                        .keys()
                        .chain(sync_result.rooms.left.keys())
                        .any(|id| {
                            dm_map
                                .as_ref()
                                .is_some_and(|m| m.values().flatten().any(|r| r == id))
                        });

                    if dm_touched {
                        let new_order = compute_dm_order(&client_sync_clone, &dm_map);
                        if new_order != prev_dm_ids {
                            prev_dm_ids = new_order.clone();
                            send_event(&handle_clone, &DmList(new_order));
                        }
                    }

                    let single_touched = sync_result
                        .rooms
                        .joined
                        .keys()
                        .chain(sync_result.rooms.left.keys())
                        .any(|id| {
                            matches!(known_room_map.get(id), Some(RoomNode::Single(_)))
                                || prev_single_ids.iter().any(|prev_id| prev_id == id.as_str())
                        });

                    if single_touched {
                        let new_order = compute_single_order(&client_sync_clone, &known_room_map);
                        if new_order != prev_single_ids {
                            prev_single_ids = new_order.clone();
                            send_event(&handle_clone, &SingleList(new_order));
                        }
                    }
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
