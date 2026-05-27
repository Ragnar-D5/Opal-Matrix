use std::time::Duration;

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
use tauri::{State, command};
use tauri_plugin_http::reqwest;
use tokio::sync::RwLock;

use crate::TauriError;

#[command(rename_all = "snake_case")]
pub(crate) async fn join_matrixrtc_call(
    matrix_client: State<'_, RwLock<Client>>,
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

    // use livekit::prelude::*;

    // let (room, mut room_events) = Room::connect(&service_url, &jwt, RoomOptions::default())
    //     .await
    //     .map_err(|e| e.to_string())?;
    // while let Some(event) = room_events.recv().await {
    //     match event {
    //         RoomEvent::TrackSubscribed {
    //             track,
    //             publication,
    //             participant,
    //         } => {
    //             debug!(
    //                 "track: {:?}, publication: {:?}, participant: {:?}",
    //                 track, publication, participant
    //             );
    //             // ...
    //         }
    //         _ => {}
    //     }
    // }

    Ok(response_json)
}
