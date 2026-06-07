use base64::Engine;
use base64::engine::general_purpose;
use livekit::e2ee::EncryptionType;
use livekit::e2ee::key_provider::{KeyProvider, KeyProviderOptions};
use log::{debug, error, info};
use matrix_sdk::deserialized_responses::ProcessedToDeviceEvent;
use matrix_sdk::event_handler::Ctx;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use futures::StreamExt;
use livekit::track::{LocalAudioTrack, LocalTrack, RemoteTrack};
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::prelude::{AudioFrame, AudioSourceOptions};
use livekit::{E2eeOptions, RoomEvent};
use matrix_sdk::ruma::MilliSecondsSinceUnixEpoch;
use matrix_sdk::ruma::api::client::discovery::discover_homeserver::RtcFocusInfo;
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
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use tauri::{AppHandle, Manager, State, command};
use tauri_plugin_http::reqwest;
use tokio::sync::RwLock;

use crate::state::{CallAudioState, LiveKitRoomData, LiveKitRoomManager};
use crate::{AsInfo, TauriError};

#[command(rename_all = "snake_case")]
pub(crate) async fn join_matrixrtc_call(
    matrix_client: State<'_, RwLock<Client>>,
    audio_state: State<'_, CallAudioState>,
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
            RtcFocusInfo::LiveKit(info) => Some(info),
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

    let host = cpal::default_host();
    let out_device = host
        .default_output_device()
        .ok_or("No default output device found")?;

    let out_config = out_device
        .default_output_config()
        .map_err(|e| format!("No output config: {}", e))?;

    let sample_rate = out_config.sample_rate(); // should be 48 000
    let channels = out_config.channels() as u32;

    log::debug!("CPAL output: {} Hz, {} ch", sample_rate, channels);

    let ring = HeapRb::<f32>::new(sample_rate as usize * channels as usize * 4);
    let (producer, mut consumer) = ring.split();
    let shared_producer = Arc::new(Mutex::new(producer));

    // ~50 ms prebuffer (sample_rate * channels / 20) before starting playback.
    let prebuffer_threshold = (sample_rate * channels / 50) as usize;
    let mut is_buffering = true;

    let output_stream = out_device
        .build_output_stream(
            &out_config.into(),
            move |data: &mut [f32], _| {
                if is_buffering {
                    if consumer.occupied_len() >= prebuffer_threshold {
                        is_buffering = false;
                    } else {
                        data.fill(0.0);
                        return;
                    }
                }
                for sample in data.iter_mut() {
                    match consumer.try_pop() {
                        Some(s) => *sample = s * 0.85,
                        None => {
                            *sample = 0.0;
                            is_buffering = true;
                        }
                    }
                }
            },
            |err| log::error!("Speaker stream error: {err}"),
            None,
        )
        .map_err(|e| format!("Failed to build output stream: {}", e))?;

    output_stream
        .play()
        .map_err(|e| format!("Failed to start output stream: {}", e))?;

    *audio_state.output_stream.lock().await = Some(output_stream);

    let in_device = host.default_input_device().ok_or("No input device found")?;
    let in_config = in_device.default_input_config()?;

    let in_sample_rate = in_config.sample_rate(); // Extracts the u32 sample rate
    let in_channels = in_config.channels() as u32;

    log::debug!("CPAL input: {} Hz, {} ch", in_sample_rate, in_channels);

    // 1. Match the WebRTC audio source parameters identically to the hardware configuration
    let native_audio_source = NativeAudioSource::new(
        AudioSourceOptions::default(),
        in_sample_rate,
        in_channels,
        10,
    );
    let local_audio_track = LocalAudioTrack::create_audio_track(
        "mic",
        livekit::RtcAudioSource::Native(native_audio_source.clone()),
    );

    // 2. Compute the precise sample count for WebRTC's strict 10ms constraint
    let samples_per_10ms = ((in_sample_rate * in_channels) / 100) as usize;

    // 3. Create a lock-free ring buffer to bridge the CPAL thread to Tokio land
    let input_ring = HeapRb::<i16>::new(samples_per_10ms * 8);
    let (mut input_producer, mut input_consumer) = input_ring.split();

    let input_stream = in_device
        .build_input_stream(
            &in_config.into(),
            move |data: &[f32], _| {
                for &sample in data {
                    let s = (sample * i16::MAX as f32) as i16;
                    // Lock-free, non-blocking push ensures zero dropouts in CPAL
                    let _ = input_producer.try_push(s);
                }
            },
            |err| log::error!("Mic stream error: {}", err),
            None,
        )
        .map_err(|e| format!("Failed to build mic stream: {}", e))?;

    input_stream
        .play()
        .map_err(|e| format!("Failed to play mic: {}", e))?;

    // Keep input stream alive in your shared state
    *audio_state.input_stream.lock().await = Some(input_stream);

    // 4. Spawn a dedicated async worker task to read frames and await WebRTC transmission
    let native_source_clone = native_audio_source.clone();
    tokio::spawn(async move {
        let mut frame_buffer = Vec::with_capacity(samples_per_10ms);
        loop {
            // Fill our accumulator up to a perfect 10ms chunk boundary
            while frame_buffer.len() < samples_per_10ms {
                if let Some(s) = input_consumer.try_pop() {
                    frame_buffer.push(s);
                } else {
                    break;
                }
            }

            // Once we have an exact 10ms block, dispatch it to WebRTC
            if frame_buffer.len() == samples_per_10ms {
                let frame = AudioFrame {
                    data: frame_buffer.clone().into(),
                    sample_rate: in_sample_rate,
                    num_channels: in_channels,
                    samples_per_channel: in_sample_rate / 100,
                };
                frame_buffer.clear();

                // Safely await here without threatening the high-priority audio pipeline
                if let Err(e) = native_source_clone.capture_frame(&frame).await {
                    log::error!("Failed to capture audio frame: {:?}", e);
                }
            } else {
                // Back off momentarily if the ring buffer is running dry to avoid pegging the CPU
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
    });

    // --- LiveKit connection ---
    let mut room_options = livekit::RoomOptions::default();
    room_options.encryption = e2ee_options;

    info!("Connecting to LiveKit room");
    let call_id = Uuid::new_v4();
    let (livekit_room, mut event_receiver) =
        livekit::Room::connect(service_url, jwt, room_options).await?;
    log::info!("Connected to LiveKit room: {:?}", livekit_room);

    // set and send out encryption key after joining but before publishig a track
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

        debug!("Trying to send encryption key to call participants");
        send_encryption_keys(client.clone(), &room_id, &local_call_key, 0, call_id).await?;
    }

    livekit_room.e2ee_manager().set_enabled(true);

    // publish mic track
    livekit_room
        .local_participant()
        .publish_track(
            LocalTrack::Audio(local_audio_track),
            livekit::options::TrackPublishOptions {
                source: livekit::track::TrackSource::Microphone,
                ..Default::default()
            },
        )
        .await
        .map_err(|e| format!("Failed to publish mic: {}", e))?;

    // persist the room in room_manager
    let cancellationtoken = CancellationToken::new();
    let cancellationtoken_clone = cancellationtoken.clone();
    let call_data = LiveKitRoomData {
        livekit_room,
        cancellation_token: cancellationtoken,
        key_index: 0,
        call_id,
    };
    livekit_room_manager
        .lock()
        .await
        .insert(room_id.clone(), call_data);

    // handle events
    let room_id_clone = room_id.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancellationtoken_clone.cancelled() => {
                    log::info!("Background room LiveKite event receiver stopped for room: {}", room_id_clone);
                    break;
                }

                // Triggered when a new room event arrives
                maybe_ev = event_receiver.recv() => {
                    match maybe_ev {
                        Some(ev) => {
                            if let RoomEvent::E2eeStateChanged { participant, state } = ev {
                                debug!("Encryption state changed for {participant:?}, new state: {state:?}");
                            } else if let RoomEvent::TrackSubscribed { track, .. } = ev
                                && let RemoteTrack::Audio(ref audio_track) = track
                            {
                                debug!("Subscribed to new audio track: {:?}", track);
                                let rtc_track = audio_track.rtc_track();

                                let mut audio_stream = NativeAudioStream::new(rtc_track, sample_rate as i32, channels as i32);
                                log::debug!("NativeAudioStream configured: {} Hz, {} ch", sample_rate, channels);

                                let prod_clone = shared_producer.clone();

                                tokio::spawn(async move {
                                    while let Some(frame) = audio_stream.next().await {
                                        let Ok(mut prod) = prod_clone.lock() else { continue; };
                                        if frame.num_channels == 1 && channels == 2 {
                                            for &s in frame.data.as_ref() {
                                                let f = s as f32 / 32_768.0;
                                                let _ = prod.try_push(f);
                                                let _ = prod.try_push(f);
                                            }
                                        } else {
                                            for &s in frame.data.as_ref() {
                                                let _ = prod.try_push(s as f32 / 32_768.0);
                                            }
                                        }
                                    }
                                });
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

    // send matrix events to signal joining the call
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

    Ok(response_json)
}

pub async fn handle_call_member_change(
    ev: OriginalSyncStateEvent<CallMemberEventContent>,
    event_room: matrix_sdk::Room,
    handle: Ctx<AppHandle>,
) {
    debug!("\n\n\n\n\n{:?}", ev.content);
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
        match getrandom::fill(&mut raw_key) {
            Err(e) => {
                error!("{e}");
                return;
            }
            _ => {}
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
            .filter(|((id, _), _)| {
                debug!(
                    "evaluating {} == {}",
                    id,
                    livekit_room.local_participant().identity()
                );
                if *id == livekit_room.local_participant().identity() {
                    true
                } else {
                    false
                }
            })
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
            debug!("Updated local call encryption key, now at indey {new_key_index}");
        }

        let client = client_state.read().await;
        debug!("Sending updated local encryption to other participants");
        match send_encryption_keys(
            client.clone(),
            event_room.room_id().as_str(),
            &local_call_key,
            new_key_index,
            call_data.call_id.clone(),
        )
        .await
        {
            Err(e) => {
                error!("{e:?}");
                return;
            }
            _ => {}
        };
    } else {
        debug!("Call member room state changed but room is not encrypted.");
    }
}

#[command(rename_all = "snake_case")]
pub(crate) async fn leave_matrixrtc_call(
    matrix_client: State<'_, RwLock<Client>>,
    audio_state: State<'_, CallAudioState>,
    livekit_room_manager: State<'_, LiveKitRoomManager>,
    room_id: String,
) -> Result<(), TauriError> {
    // close event stream and remove call from manager
    let mut data_guard = livekit_room_manager.lock().await;
    let room_data = data_guard
        .get(&room_id)
        .ok_or("Not in a call in this room".as_info())?;
    room_data.close_event_stream().await;
    room_data.livekit_room.close().await?;
    data_guard.remove(&room_id);

    *audio_state.input_stream.lock().await = None;
    *audio_state.output_stream.lock().await = None;

    let client = matrix_client.write().await;
    let room = client
        .get_room(&RoomId::parse(room_id.clone())?)
        .ok_or("Room not found or not joined")?;

    let call_content = CallMemberEventContent::new_empty(None); // use this to specify leave reason like disconnects

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
                claimed_device_id: device.device_id().to_string(),
                id: call_id.to_string(),
            },
            keys: EncryptionKeysInfo {
                index: index,
                key: key.to_string(),
            },
            session: CallSessionInfo {
                application: "m.call".to_string(),
                call_id: "".to_string(),
                scope: "m.room".to_string(),
            },
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
    debug!("Handling to-device events");
    debug!("{:?}", events);

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

        let call_data = match guard.get(&room_id).ok_or(
            "Received LiveKit key update but you are not taking part in the coresponding call",
        ) {
            Ok(tup) => tup,
            Err(e) => {
                log::info!("{e}");
                continue;
            }
        };
        debug!(
            "New key is {:?}",
            general_purpose::STANDARD
                .decode(update_event.1.keys.key.clone())
                .unwrap()
        );

        let livekit_room = &call_data.livekit_room;
        let hash = livekit_room.e2ee_manager().frame_cryptors();
        let keys = hash.keys();
        let latest_key_index = livekit_room
            .e2ee_manager()
            .key_provider()
            .unwrap()
            .get_latest_key_index();
        debug!("\n\n\n\nFramecryptors available for {:?}", keys);
        for key in keys {
            debug!(
                "Key associated with {:?} is {:?}",
                key.0,
                livekit_room
                    .e2ee_manager()
                    .key_provider()
                    .unwrap()
                    .get_key(&key.0, latest_key_index)
            );
        }

        let e2ee_manager = livekit_room.e2ee_manager();

        let key_provider = match e2ee_manager.key_provider().ok_or("No key provider found") {
            Ok(k) => k,
            Err(e) => {
                log::info!("{e}");
                continue;
            }
        };

        debug!("\n{:?}\n", e2ee_manager.frame_cryptors().keys());

        let livekit_id = format!(
            "{}:{}",
            update_event.0, update_event.1.member.claimed_device_id
        );

        key_provider.set_key(
            &From::from(livekit_id.clone()),
            update_event.1.keys.index,
            general_purpose::STANDARD
                .decode(update_event.1.keys.key)
                .unwrap(),
        );
        log::info!("Saved updated LiveKit encryption key for {}", livekit_id);

        e2ee_manager
            .frame_cryptors()
            .iter()
            .filter(|((id, _), _)| {
                if id == &From::from(livekit_id.clone()) {
                    true
                } else {
                    false
                }
            })
            .for_each(|((id, _), frame_cryptor)| {
                frame_cryptor.set_key_index(update_event.1.keys.index);
                debug!("Updated FrameCryptor key index for {id}");
            });

        let hash = livekit_room.e2ee_manager().frame_cryptors();
        let keys = hash.keys();
        let latest_key_index = livekit_room
            .e2ee_manager()
            .key_provider()
            .unwrap()
            .get_latest_key_index();
        debug!("\n\n\n\nFramecryptors available for {:?}", keys);
        for key in keys {
            debug!(
                "Key associated with {:?} is {:?}",
                key.0,
                livekit_room
                    .e2ee_manager()
                    .key_provider()
                    .unwrap()
                    .get_key(&key.0, latest_key_index)
            );
        }
    }

    debug!("Finished handling to-device events");
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
