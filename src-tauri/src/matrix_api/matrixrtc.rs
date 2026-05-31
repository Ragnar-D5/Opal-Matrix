use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use futures::StreamExt;
use livekit::track::RemoteTrack;
use livekit::webrtc::audio_stream::native::NativeAudioStream;
use livekit::{PlatformAudio, RoomEvent};
use log::debug;
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
    // Changed return type to pass the JWT data back
    log::info!("Started Call");

    // Read the client lock once for setup data
    let client = matrix_client.read().await;

    // Get the device_id from your current active Matrix session
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

    // Drop the read guard before initiating outbound network requests

    // 2. Format the payload EXACTLY like Element Web
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

    log::debug!("Sending payload: {:?}", auth_payload);

    // 3. POST the payload to the SFU server
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

    // 4. Parse the working {"url": "...", "jwt": "..."} response
    let response_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse SFU response JSON: {}", e))?;

    log::info!("Successfully acquired LiveKit token!");

    let service_url = response_json["url"].as_str().ok_or("No url returned")?;
    let jwt = response_json["jwt"].as_str().ok_or("No jwt returned")?;

    debug!("url: {service_url}, token: {jwt}");

    let room = client
        .get_room(&RoomId::parse(room_id.clone())?)
        .ok_or("Room not found or not joined")?;

    // // 3. Define your Call Properties
    // // 'call_id' is a unique string identifying this specific session.
    // let call_id = "main_room_video_call";

    let call_content = CallMemberEventContent::new(
        Application::Call(CallApplicationContent::new(
            "".to_string(),
            matrix_sdk::ruma::events::call::member::CallScope::Room,
        )),
        client.device_id().ok_or("No DeviceId")?.into(),
        matrix_sdk::ruma::events::call::member::ActiveFocus::Livekit(ActiveLivekitFocus::new()),
        vec![Focus::Livekit(LivekitFocus::new(
            room_id,
            service_url.to_string(),
        ))],
        None,
        None,
    );

    // // 4. Send the State Event
    // // MatrixRTC state events require a state_key, which is usually set to the Call ID string.
    // println!("Signaling modern group call start...");

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

    let audio = PlatformAudio::new()?;

    let (livekit_room, mut event_receiver) =
        livekit::Room::connect(&service_url, &jwt, livekit::RoomOptions::default()).await?;
    log::info!("Connected to room: {:?}", livekit_room);

    let ring = HeapRb::<i16>::new(48000 * 4); // Large enough lock-free buffer
    let (producer, mut consumer) = ring.split();
    let shared_producer = Arc::new(Mutex::new(producer));

    let host = cpal::default_host();

    let mut out_channels = None;
    let mut out_sample_rate = None;

    if let Some(out_device) = host.default_output_device() {
        if let Ok(out_config) = out_device.default_output_config() {
            out_channels = Some(out_config.channels());
            out_sample_rate = Some(out_config.sample_rate());
            let prebuffer_threshold =
                (out_channels.unwrap() as u32 * out_sample_rate.unwrap()) / 20;

            let mut is_buffering = true;

            let output_stream = out_device
                .build_output_stream(
                    &out_config.into(),
                    move |data: &mut [f32], _| {
                        // 1. Wait for the safety net to fill before playing
                        if is_buffering {
                            if consumer.occupied_len() >= prebuffer_threshold as usize {
                                is_buffering = false; // We have enough audio, start playing!
                            } else {
                                // Output silence while buffering
                                for sample in data.iter_mut() {
                                    *sample = 0.0;
                                }
                                return;
                            }
                        }

                        // 2. Play the audio
                        for sample in data.iter_mut() {
                            match consumer.try_pop() {
                                Some(pcm) => {
                                    *sample = pcm as f32 / i16::MAX as f32;
                                }
                                None => {
                                    // Underflow! We completely ran out of audio.
                                    // Play silence and go back into buffering mode to rebuild the safety net.
                                    *sample = 0.0;
                                    is_buffering = true;
                                }
                            }
                        }
                    },
                    |err| log::error!("Speaker stream error: {}", err),
                    None,
                )
                .map_err(|e| format!("Failed to build speaker stream: {}", e))?;

            output_stream
                .play()
                .map_err(|e| format!("Failed to play speakers: {}", e))?;

            // Store stream to keep it alive
            *audio_state.output_stream.lock().await = Some(output_stream);
        }
    }

    let target_channels = out_channels.unwrap_or(2) as u32;

    tokio::spawn(async move {
        while let Some(ev) = event_receiver.recv().await {
            log::info!("Recieved livekit event: {:?}", ev);
            match ev {
                RoomEvent::TrackSubscribed {
                    track,
                    publication,
                    participant,
                } => {
                    match track {
                        RemoteTrack::Audio(audio_track) => {
                            let rtc_track = audio_track.rtc_track();
                            let mut audio_stream = NativeAudioStream::new(
                                rtc_track,
                                out_sample_rate.unwrap_or(48000) as i32,
                                out_channels.unwrap_or(2) as i32,
                            );

                            let prod_clone = shared_producer.clone();

                            tokio::spawn(async move {
                                while let Some(audio_frame) = audio_stream.next().await {
                                    // Push Livekit audio samples into our shared OS speaker queue
                                    if let Ok(mut prod) = prod_clone.lock() {
                                        let actual_channels = audio_frame.num_channels;

                                        if actual_channels == 1 && target_channels == 2 {
                                            // FIX: Upmix Mono to Stereo by duplicating each sample
                                            for &sample in audio_frame.data.as_ref() {
                                                let _ = prod.try_push(sample); // Left ear
                                                let _ = prod.try_push(sample); // Right ear
                                            }
                                        } else if actual_channels == 2 && target_channels == 1 {
                                            // FIX: Downmix Stereo to Mono by averaging L and R
                                            let data = audio_frame.data.as_ref();
                                            for chunk in data.chunks_exact(2) {
                                                let mixed = ((chunk[0] as i32 + chunk[1] as i32)
                                                    / 2)
                                                    as i16;
                                                let _ = prod.try_push(mixed);
                                            }
                                        } else {
                                            // Channels match, push everything at once
                                            let _ = prod.push_slice(audio_frame.data.as_ref());
                                        }
                                    }
                                }
                            });
                        }
                        _ => (),
                    }
                }
                _ => (),
            }
        }
    });

    Ok(response_json)
}
