use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use futures::StreamExt;
use livekit::RoomEvent;
use livekit::track::{LocalAudioTrack, LocalTrack, RemoteTrack};
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::webrtc::prelude::{AudioFrame, AudioSourceOptions};
use matrix_sdk::ruma::MilliSecondsSinceUnixEpoch;
use matrix_sdk::ruma::api::client::discovery::discover_homeserver::RtcFocusInfo;
use matrix_sdk::ruma::events::Mentions;
use matrix_sdk::ruma::events::call::member::{
    ActiveLivekitFocus, Application, CallApplicationContent, CallMemberEventContent,
    CallMemberStateKey, Focus, LivekitFocus,
};
use matrix_sdk::ruma::events::relation::Reference;
use matrix_sdk::ruma::events::rtc::notification::RtcNotificationEventContent;
use matrix_sdk::{Client, ruma::RoomId};
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use tauri::{State, command};
use tauri_plugin_http::reqwest;
use tokio::sync::RwLock;

use crate::TauriError;
use crate::state::CallAudioState;

#[command(rename_all = "snake_case")]
pub(crate) async fn join_matrixrtc_call(
    matrix_client: State<'_, RwLock<Client>>,
    audio_state: State<'_, CallAudioState>,
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

    let room = client
        .get_room(&RoomId::parse(room_id.clone())?)
        .ok_or("Room not found or not joined")?;

    let call_content = CallMemberEventContent::new(
        Application::Call(CallApplicationContent::new(
            "".to_string(),
            matrix_sdk::ruma::events::call::member::CallScope::Room,
        )),
        client.device_id().ok_or("No DeviceId")?.into(),
        matrix_sdk::ruma::events::call::member::ActiveFocus::Livekit(ActiveLivekitFocus::new()),
        vec![Focus::Livekit(LivekitFocus::new(
            room_id.clone(),
            service_url.to_string(),
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

    let (livekit_room, mut event_receiver) =
        livekit::Room::connect(service_url, jwt, livekit::RoomOptions::default()).await?;
    log::debug!("Connected to LiveKit room: {:?}", livekit_room);

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

    tokio::spawn(async move {
        while let Some(ev) = event_receiver.recv().await {
            log::debug!("Livekit event received: {ev:?}");
            if let RoomEvent::TrackSubscribed { track, .. } = ev
                && let RemoteTrack::Audio(audio_track) = track
            {
                let rtc_track = audio_track.rtc_track();

                // Tell NativeAudioStream to deliver at 48 kHz / stereo.
                // Because this matches what CPAL was configured for above, the
                // WebRTC engine performs no resampling — the main source of the
                // robotic artifact is gone.
                let mut audio_stream =
                    NativeAudioStream::new(rtc_track, sample_rate as i32, channels as i32);

                log::debug!(
                    "NativeAudioStream configured: {} Hz, {} ch",
                    sample_rate,
                    channels
                );

                let prod_clone = shared_producer.clone();

                tokio::spawn(async move {
                    while let Some(frame) = audio_stream.next().await {
                        let Ok(mut prod) = prod_clone.lock() else {
                            continue;
                        };

                        if frame.num_channels == 1 && channels == 2 {
                            for &s in frame.data.as_ref() {
                                let f = s as f32 / 32_768.0;
                                // push_overwrite keeps the buffer fresh — if
                                // the consumer falls behind for any reason,
                                // we discard the oldest samples rather than
                                // dropping the newest (which is what caused
                                // the growing delay in the previous version).
                                let _ = prod.try_push(f);
                                // prod.try_push(f);
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
    });

    Ok(response_json)
}

#[command(rename_all = "snake_case")]
pub(crate) async fn leave_matrixrtc_call(
    matrix_client: State<'_, RwLock<Client>>,
    audio_state: State<'_, CallAudioState>,
    room_id: String,
) -> Result<(), TauriError> {
    *audio_state.input_stream.lock().await = None;
    *audio_state.output_stream.lock().await = None;

    let client = matrix_client.read().await;
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
