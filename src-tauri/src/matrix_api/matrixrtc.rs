use base64::Engine;
use base64::engine::general_purpose;
use livekit::e2ee::EncryptionType;
use livekit::e2ee::key_provider::{KeyProvider, KeyProviderOptions};
use log::{debug, error, info, warn};
use matrix_sdk::deserialized_responses::ProcessedToDeviceEvent;
use matrix_sdk::event_handler::Ctx;
use matrix_sdk::ruma::api::client::rtc::RtcTransport;
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use cpal::traits::{DeviceTrait, HostTrait};
use futures::StreamExt;
use livekit::track::{LocalAudioTrack, LocalTrack, RemoteAudioTrack, RemoteTrack};
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::{E2eeOptions, RoomEvent};
use matrix_sdk::ruma::MilliSecondsSinceUnixEpoch;
use matrix_sdk::ruma::events::call::member::{
    ActiveLivekitFocus, Application, CallApplicationContent, CallMemberEventContent,
    CallMemberStateKey, Focus, LivekitFocus,
};
use matrix_sdk::ruma::events::relation::Reference;
use matrix_sdk::ruma::events::rtc::notification::RtcNotificationEventContent;
use matrix_sdk::ruma::events::{
    AnyStateEventContent, AnyToDeviceEventContent, Mentions, OriginalSyncStateEvent, StateEventType,
};
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::{Client, ruma::RoomId};
use ringbuf::HeapCons;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use tauri::{AppHandle, Manager, State, command};
use tauri_plugin_http::reqwest;
use tokio::sync::RwLock;

use crate::state::{AudioManager, LiveKitRoomData, LiveKitRoomManager};
use crate::{LogResultExt, TauriError};

#[command(rename_all = "snake_case")]
pub(crate) async fn join_matrixrtc_call(
    handle: AppHandle,
    matrix_client: State<'_, RwLock<Client>>,
    audio_manager: State<'_, AudioManager>,
    livekit_room_manager: State<'_, LiveKitRoomManager>,
    room_id: String,
) -> Result<serde_json::Value, TauriError> {
    log::info!("Started joining call");

    let client = matrix_client.read().await;

    let device_id = client
        .device_id()
        .map(|d| d.to_string())
        .ok_or_else(|| "Matrix client is not logged in or missing a device_id".to_string())?;

    let rtc_foci = client
        .rtc_foci()
        .await
        .map_err(|e| format!("Failed to get RTC foci: {}", e))?;

    let default_livekit_focus_info = rtc_foci
        .iter()
        .find_map(|focus| match focus {
            RtcTransport::LiveKit(info) => Some(info),
            _ => None,
        })
        .ok_or_else(|| "No rtc focus information found".to_string())?;

    let jwt_url = default_livekit_focus_info.service_url.clone() + "/sfu/get";

    let openid_token = matrix_sdk::Account::request_openid_token(&client.account())
        .await
        .map_err(|e| format!("OpenID token request failed: {}", e))?;

    let auth_payload = serde_json::json!({
        "room": room_id,
        "openid_token": {
            "access_token": openid_token.access_token,
            "expires_in": openid_token.expires_in.as_secs(),
            "matrix_server_name": openid_token.matrix_server_name,
            "token_type": openid_token.token_type.to_string(),
        },
        "device_id": device_id
    });

    let http_client = reqwest::Client::new();
    let res = http_client
        .post(&jwt_url)
        .json(&auth_payload)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let err_body = res.text().await.unwrap_or_default();
        return Err(format!("SFU Server rejected request ({}): {}", status, err_body).into());
    }

    let response_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse SFU response JSON: {}", e))?;

    let service_url = response_json["url"].as_str().ok_or("No url returned")?;
    let jwt = response_json["jwt"].as_str().ok_or("No jwt returned")?;

    log::info!("Successfully acquired LiveKit token!");

    let mut e2ee_options = None;

    let parsed_room_id =
        RoomId::parse(&room_id).map_err(|e| format!("Invalid Room ID format: {}", e))?;

    let room = client
        .get_room(&parsed_room_id)
        .ok_or("Room not found or not joined")?;

    if room.encryption_state().is_encrypted() {
        let key_provider = KeyProvider::new(KeyProviderOptions {
            key_ring_size: 128,
            key_derivation_algorithm: livekit::e2ee::key_provider::KeyDerivationAlgorithm::HKDF,
            ..Default::default()
        });

        e2ee_options = Some(E2eeOptions {
            encryption_type: EncryptionType::Gcm,
            key_provider,
        });
    }

    // LiveKit connection
    let cancellationtoken = CancellationToken::new();
    let mut room_options = livekit::RoomOptions::default();
    room_options.encryption = e2ee_options;

    info!("Connecting to LiveKit room");
    let call_id = Uuid::new_v4();
    let (livekit_room, mut event_receiver) =
        livekit::Room::connect(service_url, jwt, room_options).await?;
    log::info!("Connected to LiveKit room: {:?}", livekit_room);

    // Set up local encryption key
    let mut local_call_key_opt = None;
    if room.encryption_state().is_encrypted() {
        debug!("Room is encrypted. Generating encryption key");
        let mut raw_key = [0u8; 16];
        getrandom::fill(&mut raw_key)
            .map_err(|e| format!("Failed to generate cryptographic key: {}", e))?;

        let local_call_key = general_purpose::STANDARD.encode(raw_key);

        let key_provider = livekit_room
            .e2ee_manager()
            .key_provider()
            .expect("Keyprovider already set");
        key_provider.set_key(
            &livekit_room.local_participant().identity(),
            0,
            raw_key.into(),
        );
        debug!("Set encryption key for local participant.");
        local_call_key_opt = Some(local_call_key);
    }

    livekit_room.e2ee_manager().set_enabled(true);

    // persist in room manager now so incoming to-device messages aren't rejected
    let cancellationtoken_clone = cancellationtoken.clone();
    let call_data = LiveKitRoomData {
        livekit_room,
        cancellation_token: cancellationtoken_clone,
        key_index: 0,
        call_id,
    };
    livekit_room_manager
        .lock()
        .await
        .insert(room_id.clone(), call_data);

    // Send encryption keys to existing participants
    if let Some(local_call_key) = local_call_key_opt {
        debug!("Trying to send encryption key to call participants");
        send_encryption_keys(client.clone(), &room_id, &local_call_key, 0, call_id).await?;
    }

    // announce joining to matrix room now that we are ready to receive keys
    let call_content = CallMemberEventContent::new(
        Application::Call(CallApplicationContent::new(
            "".to_string(),
            matrix_sdk::ruma::events::call::member::CallScope::Room,
        )),
        client.device_id().ok_or("No DeviceId")?.into(),
        matrix_sdk::ruma::events::call::member::ActiveFocus::Livekit(ActiveLivekitFocus::new()),
        vec![Focus::Livekit(LivekitFocus::new(
            room_id.clone(),
            default_livekit_focus_info.service_url.to_string(),
        ))],
        None,
        None,
    );

    let response = room
        .send_state_event_for_key(
            &CallMemberStateKey::new(
                client.user_id().ok_or("No UserId")?.into(),
                Some(client.device_id().ok_or("No DeviceId")?.into()),
                true,
            ),
            call_content,
        )
        .await?;

    let mut notification_event = RtcNotificationEventContent::new(
        MilliSecondsSinceUnixEpoch::now(),
        Duration::from_mins(1),
        matrix_sdk::ruma::events::rtc::notification::NotificationType::Ring,
    );
    notification_event.mentions = Some(Mentions::with_room_mention());
    notification_event.call_intent =
        Some(matrix_sdk::ruma::events::rtc::notification::CallIntent::Audio);
    notification_event.relates_to = Some(Reference::new(response.event_id));

    room.send(notification_event).await?;

    // Audio setup
    if let Some(device) = audio_manager.host.default_output_device() {
        if let Err(e) = audio_manager.try_setup_output_stream_for_device(&device) {
            warn!("Could not set up output stream: {:?}", e);
        } else {
            debug!("Set up cpal output stream");
        }
    } else {
        warn!("No default output device found");
    }

    if let Some(device) = audio_manager.host.default_input_device() {
        if let Err(e) = audio_manager.try_setup_input_stream_for_device(&device) {
            warn!("Could not set up input stream: {:?}", e);
        } else {
            debug!("Set up input stream");
        }
    } else {
        warn!("No default input device found");
    }

    let microphone_track = setup_mic_track(&audio_manager);

    // Publish microphone track
    if let Ok(track) = microphone_track {
        let mut manager = livekit_room_manager.lock().await;
        let call_data = manager.get_mut(&room_id.clone()).unwrap();
        call_data
            .livekit_room
            .local_participant()
            .publish_track(
                LocalTrack::Audio(track),
                livekit::options::TrackPublishOptions {
                    source: livekit::track::TrackSource::Microphone,
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| format!("Failed to publish mic: {}", e))?;
        debug!("Published microphone track");
    } else {
        warn!(
            "Could not set up microphone track: {}",
            microphone_track.unwrap_err()
        )
    }

    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

    // Spawn main audio mixer
    if let Some(producer) = audio_manager.output_producer.lock().unwrap().take() {
        log::debug!("Spawning audio mixer");
        spawn_audio_mixer(producer, receiver, cancellationtoken.clone());
    } else {
        log::error!("output_producer is None - mixer not spawned, remote audio will be silent");
    }

    let room_id_clone = room_id.clone();
    let cancellationtoken_clone = cancellationtoken.clone();
    let livekit_room_manager_inner = livekit_room_manager.inner().clone();

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancellationtoken_clone.cancelled() => {
                    log::info!("Background room LiveKit event receiver stopped for room: {}", room_id_clone);
                    break;
                }

                maybe_ev = event_receiver.recv() => {
                    match maybe_ev {
                        Some(ev) => {
                            if let RoomEvent::E2eeStateChanged { participant, state } = ev {
                                debug!("Encryption state changed for {participant:?}, new state: {state:?}");
                            } else if let RoomEvent::TrackSubscribed { track, participant, .. } = ev
                                && let RemoteTrack::Audio(ref audio_track) = track
                            {
                                debug!("Subscribed to new audio track: {:?}", track);

                                // Use the cloned inner manager here
                                let manager = livekit_room_manager_inner.lock().await;
                                if let Some(call_data) = manager.get(&room_id_clone) {
                                    let e2ee_mgr = call_data.livekit_room.e2ee_manager();
                                    for ((id, _), fc) in e2ee_mgr.frame_cryptors() {
                                        if id == participant.identity() {
                                            fc.set_key_index(call_data.key_index);
                                        }
                                    }
                                }

                                if let Err(e) = register_new_track(
                                    handle.clone(),
                                    audio_track,
                                    sender.clone(),
                                    cancellationtoken_clone.clone(),
                                ).await {
                                    warn!("Could not register output of remote track: {e}")
                                }
                            }
                        }
                        None => {
                            log::info!("LiveKit event channel closed by remote host.");
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok(response_json)
}

pub async fn handle_call_member_change(
    ev: OriginalSyncStateEvent<CallMemberEventContent>,
    event_room: matrix_sdk::Room,
    handle: Ctx<AppHandle>,
) {
    if let CallMemberEventContent::LegacyContent(_) = ev.content {
        return;
    }

    let livekit_room_manager_guard = handle
        .try_state::<LiveKitRoomManager>()
        .expect("Could not aquire LiveKitRoomManager from State. Likely an implementation error.");
    let mut livekit_room_manager = livekit_room_manager_guard.lock().await;

    let Some(call_data) = livekit_room_manager.get_mut(event_room.room_id().as_str()) else {
        debug!("Call member room state changed, but you are not part of the call.");
        return;
    };
    debug!("Call members changed in room {}", event_room.room_id());

    if event_room.encryption_state().is_encrypted() {
        debug!("Room is encrypted");

        let client_state = handle
            .try_state::<RwLock<Client>>()
            .expect("Could not acquire Client from State. Likely an implementation error.");

        let mut raw_key = [0u8; 16];
        if let Err(e) = getrandom::fill(&mut raw_key) {
            log::error!("{e}");
            return;
        };

        let local_call_key = general_purpose::STANDARD.encode(raw_key);
        let livekit_room = &call_data.livekit_room;
        let e2ee_manager = livekit_room.e2ee_manager();

        let key_provider = call_data
            .livekit_room
            .e2ee_manager()
            .key_provider()
            .expect("Ecrypted LiveKit room without key provider");

        call_data.key_index += 1;
        let new_key_index = call_data.key_index;

        key_provider.set_key(
            &call_data.livekit_room.local_participant().identity(),
            new_key_index,
            raw_key.into(),
        );
        let all_framecryptors = e2ee_manager.frame_cryptors();
        let local_framecryptors: Vec<_> = all_framecryptors
            .iter()
            .filter(|((id, _), _)| *id == livekit_room.local_participant().identity())
            .collect();
        if local_framecryptors.is_empty() {
            error!("No FrameCryptors found for local participant.");
        } else {
            local_framecryptors
                .iter()
                .for_each(|((_, track), frame_cryptor)| {
                    frame_cryptor.set_key_index(new_key_index);
                    debug!("Updated local FrameCryptor key index for track {track}");
                });
            debug!("Updated local call encryption key, now at index {new_key_index}");
        }

        let client = client_state.read().await;
        debug!("Sending updated local encryption to other participants");
        if let Err(e) = send_encryption_keys(
            client.clone(),
            event_room.room_id().as_str(),
            &local_call_key,
            new_key_index,
            call_data.call_id,
        )
        .await
        {
            log::error!("{e:?}");
        };
    } else {
        debug!("Call member room state changed but room is not encrypted.");
    }
}

#[command(rename_all = "snake_case")]
pub(crate) async fn leave_matrixrtc_call(
    matrix_client: State<'_, RwLock<Client>>,
    livekit_room_manager: State<'_, LiveKitRoomManager>,
    room_id: String,
) -> Result<(), TauriError> {
    // close event stream and remove call from manager
    let mut data_guard = livekit_room_manager.lock().await;
    let room_data = data_guard
        .get(&room_id)
        .ok_or("Not in a call in this room")
        .log_as_info()?;
    room_data.close_event_stream().await;
    room_data.livekit_room.close().await?;
    data_guard.remove(&room_id);

    let client = matrix_client.write().await;
    let room = client
        .get_room(&RoomId::parse(room_id.clone())?)
        .ok_or("Room not found or not joined")?;

    let call_content = CallMemberEventContent::new_empty(None);

    let _response = room
        .send_state_event_for_key(
            &CallMemberStateKey::new(
                client.user_id().ok_or("No UserId")?.into(),
                Some(client.device_id().ok_or("No DeviceId")?.into()),
                true,
            ),
            call_content,
        )
        .await?;

    Ok(())
}

use serde::{Deserialize, Serialize};

async fn send_encryption_keys(
    client: Client,
    room_id: &str,
    key: &str,
    index: i32,
    call_id: Uuid,
) -> Result<(), TauriError> {
    debug!("Sending encryption key to call participants");
    let room = client
        .get_room(&RoomId::parse(room_id)?)
        .ok_or("Room not found or not joined")?;

    let own_id = client
        .user_id()
        .expect("Should be set when you try to join a call");
    let own_device = client
        .device_id()
        .expect("Should be set when you try to join a call");

    let state_events = room.get_state_events(StateEventType::CallMember).await?;

    for raw_state in state_events {
        let event = match raw_state.deserialize() {
            Ok(ev) => ev,
            Err(e) => {
                log::error!("Failed to deserialize event: {:?}", e);
                continue;
            }
        };
        let Some(event) = event.as_sync() else {
            continue;
        };
        let sender = event.sender();
        let Some(event) = event.original_content() else {
            continue;
        };
        let AnyStateEventContent::CallMember(event) = event else {
            continue;
        };
        let CallMemberEventContent::SessionContent(content) = event else {
            continue;
        };

        if sender == own_id && own_device == content.device_id {
            continue;
        }

        let device = match client
            .encryption()
            .get_device(sender, &content.device_id)
            .await
        {
            Ok(opt) => match opt {
                Some(device) => device,
                None => {
                    info!(
                        "The device {} of user {} is in the call, but not logged in. Skipping in call encryption key distribution.",
                        content.device_id, sender
                    );
                    continue;
                }
            },
            Err(e) => {
                error!(
                    "Error while getting device {} for user {} from crypto store: {e}",
                    content.device_id, sender
                );
                continue;
            }
        };

        let payload = EncryptionKeysEventContent {
            room_id: room.room_id().to_string(),
            member: CallMemberInfo {
                claimed_device_id: own_device.to_string(),
                id: call_id.to_string(),
            },
            keys: EncryptionKeysInfo {
                index,
                key: key.to_string(),
            },
            session: CallSessionInfo::default(),
            sent_ts: MilliSecondsSinceUnixEpoch::now(),
        };

        let json_str = serde_json::to_string(&payload).expect("Failed to serialize payload");

        let raw_payload: Raw<AnyToDeviceEventContent> =
            serde_json::from_str(&json_str).expect("Failed to create Raw event");

        client
            .encryption()
            .encrypt_and_send_raw_to_device(
                vec![&device],
                "io.element.call.encryption_keys",
                raw_payload,
                Default::default(),
            )
            .await
            .map_err(|e| {
                format!(
                    "Error when sending call encryption key to device {} of user {}: {e}",
                    content.device_id, sender
                )
            })?;
    }

    log::info!("Finished distributing call encryption key to participants.");

    Ok(())
}

pub async fn cleanup_ghost_calls(client: &matrix_sdk::Client) {
    let Some(device_id) = client.device_id() else {
        return;
    };
    let Some(user_id) = client.user_id() else {
        return;
    };

    let state_key = format!("_{}_{}", user_id, device_id);

    for room in client.joined_rooms() {
        if let Ok(Some(raw_event)) = room
            .get_state_event(
                matrix_sdk::ruma::events::StateEventType::CallMember,
                &state_key,
            )
            .await
        {
            if let Ok(json_event) = serde_json::to_value(&raw_event)
                && let Some(content) = json_event.get("content")
                && content.as_object().is_some_and(|obj| obj.is_empty())
            {
                continue;
            }

            log::debug!("Cleaning up ghost participant in room: {}", room.room_id());

            let _ = room
                .send_state_event_raw("m.call.member", &state_key, serde_json::json!({}))
                .await;
        }
    }
}

/// currently only handles call key updates
pub async fn handle_to_device_messages(
    events: Vec<ProcessedToDeviceEvent>,
    app_handle: AppHandle,
) -> Result<(), TauriError> {
    let key_updates = events
        .into_iter()
        .filter_map(|e| {
            if let ProcessedToDeviceEvent::Decrypted { raw, .. } = e {
                if raw.get_field::<String>("type").ok()?.as_deref()
                    == Some("io.element.call.encryption_keys")
                {
                    let sender = raw.get_field("sender").unwrap().unwrap();
                    let json_value: serde_json::Value = raw.deserialize_as().ok()?;
                    let content_json = json_value.get("content")?.clone();
                    let content: EncryptionKeysEventContent =
                        serde_json::from_value(content_json).ok()?;
                    Some((sender, content))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect::<Vec<(String, EncryptionKeysEventContent)>>();

    let state = app_handle
        .try_state::<LiveKitRoomManager>()
        .ok_or("Couldn't aquire LiveKitRoomManager from State")?;
    let guard = state.lock().await;

    for update_event in key_updates {
        let room_id = update_event.1.room_id.clone();

        let livekit_id = format!(
            "{}:{}",
            update_event.0, update_event.1.member.claimed_device_id
        );

        debug!("Handling key update for {livekit_id}");

        let call_data = match guard.get(&room_id).ok_or(
            "Received LiveKit key update but you are not taking part in the coresponding call",
        ) {
            Ok(tup) => tup,
            Err(e) => {
                log::info!("{e}");
                continue;
            }
        };

        let livekit_room = &call_data.livekit_room;

        let e2ee_manager = livekit_room.e2ee_manager();

        let key_provider = match e2ee_manager.key_provider().ok_or("No key provider found") {
            Ok(k) => k,
            Err(e) => {
                log::info!("{e}");
                continue;
            }
        };

        key_provider.set_key(
            &From::from(livekit_id.clone()),
            update_event.1.keys.index,
            general_purpose::STANDARD
                .decode(update_event.1.keys.key)
                .unwrap(),
        );
        log::info!(
            "Set updated LiveKit decryption key for {} with index {} in KeyProvider.",
            livekit_id,
            update_event.1.keys.index
        );

        e2ee_manager
            .frame_cryptors()
            .iter()
            .filter(|((id, _), _)| id == &From::from(livekit_id.clone()))
            .for_each(|((id, _), frame_cryptor)| {
                frame_cryptor.set_key_index(update_event.1.keys.index);
                debug!("Updated FrameCryptor key index for {id}");
            });
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EncryptionKeysEventContent {
    pub keys: EncryptionKeysInfo,
    pub member: CallMemberInfo,
    pub room_id: String,
    pub sent_ts: MilliSecondsSinceUnixEpoch,
    pub session: CallSessionInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EncryptionKeysInfo {
    pub index: i32,
    pub key: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CallMemberInfo {
    pub claimed_device_id: String,
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CallSessionInfo {
    pub application: String,
    pub call_id: String,
    pub scope: String,
}

impl Default for CallSessionInfo {
    fn default() -> Self {
        CallSessionInfo {
            application: "m.call".to_string(),
            call_id: "".to_string(),
            scope: "m.room".to_string(),
        }
    }
}

use anyhow::Context;

pub fn setup_mic_track(audio_manager: &AudioManager) -> Result<LocalAudioTrack, anyhow::Error> {
    // Cloning the source here — if setup_global_input_handler hasn't run yet,
    // this will be the default source (48000/2) and the handler will later replace
    // it with a new object, breaking the link to this track.
    log::debug!(
        "setup_mic_track: cloning NativeAudioSource to create track (handler may not have replaced it yet)"
    );
    let source = audio_manager.native_audio_source.lock().unwrap().clone();

    let track =
        LocalAudioTrack::create_audio_track("Microphone", livekit::RtcAudioSource::Native(source));

    log::debug!("setup_mic_track: track created");
    Ok(track)
}

pub fn spawn_audio_mixer(
    mut master_producer: ringbuf::HeapProd<f32>,
    mut new_track_receiver: UnboundedReceiver<ringbuf::HeapCons<f32>>,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        let mut active_tracks: Vec<ringbuf::HeapCons<f32>> = Vec::new();

        loop {
            // Drain incoming new track registrations
            while let Ok(new_consumer) = new_track_receiver.try_recv() {
                active_tracks.push(new_consumer);
            }

            // Perform the mixing
            let spaces_available = master_producer.vacant_len();
            for _ in 0..spaces_available {
                let mut mixed_sample = 0.0;
                let mut active_speakers = 0;

                active_tracks.retain_mut(|consumer| {
                    if let Some(sample) = consumer.try_pop() {
                        mixed_sample += sample;
                        active_speakers += 1;
                        true
                    } else {
                        true
                    }
                });

                if active_speakers > 0 {
                    let final_sample = mixed_sample.clamp(-1.0, 1.0);
                    let _ = master_producer.try_push(final_sample);
                } else {
                    let _ = master_producer.try_push(0.0);
                }
            }

            // Race the 2ms sleep against the cancellation token
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    log::info!("Central Audio Mixer task received cancellation. Shutting down loop.");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(2)) => {}
            }
        }
    });
}

async fn register_new_track(
    handle: AppHandle,
    audio_track: &RemoteAudioTrack,
    sender: UnboundedSender<HeapCons<f32>>,
    cancel: CancellationToken,
) -> Result<(), anyhow::Error> {
    let audio_manager = handle.state::<AudioManager>();
    let output_device = audio_manager
        .output_device
        .lock()
        .unwrap()
        .clone()
        .context("No output device set")?;
    let config = output_device.default_output_config()?;
    let sample_rate = config.sample_rate();
    let channels = config.channels() as u32;
    let rtc_track = audio_track.rtc_track();
    let mut audio_stream = NativeAudioStream::new(rtc_track, sample_rate as i32, channels as i32);

    let track_ring = ringbuf::HeapRb::<f32>::new(((sample_rate * channels) / 50) as usize);
    let (mut track_producer, track_consumer) = track_ring.split();

    // Pass the consumer end to the mixer
    match sender.send(track_consumer) {
        Ok(_) => log::debug!("register_new_track: sent consumer to mixer"),
        Err(_) => log::error!(
            "register_new_track: mixer receiver dropped — mixer was not spawned or already shut down"
        ),
    }

    tokio::spawn(async move {
        let mut frame_count = 0u64;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    log::info!("Participant download task aborted via call cancellation.");
                    break;
                }
                maybe_frame = audio_stream.next() => {
                    match maybe_frame {
                        Some(frame) => {
                            if frame_count == 0 {
                                log::debug!("register_new_track: first audio frame received ({}Hz, {} samples)", frame.sample_rate, frame.data.len());
                            }
                            frame_count += 1;
                            for &s in frame.data.as_ref() {
                                let f = s as f32 / 32_768.0;
                                let _ = track_producer.try_push(f);
                            }
                        }
                        None => {
                            log::info!("Participant audio stream ended by remote host.");
                            break;
                        }
                    }
                }
            }
        }
    });
    Ok(())
}
