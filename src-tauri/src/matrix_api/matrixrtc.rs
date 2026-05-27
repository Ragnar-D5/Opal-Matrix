use log::debug;
use matrix_sdk::Client;
use ruma::api::client::discovery::discover_homeserver::RtcFocusInfo;
use tauri::{State, command};
use tauri_plugin_http::reqwest;
use tokio::sync::RwLock;

#[command(rename_all = "snake_case")]
pub(crate) async fn join_matrixrtc_call(
    matrix_client: State<'_, RwLock<Client>>,
    room_id: String,
) -> Result<serde_json::Value, String> {
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
    drop(client);

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
        return Err(format!(
            "SFU Server rejected request ({}): {}",
            status, err_body
        ));
    }

    // 4. Parse the working {"url": "...", "jwt": "..."} response
    let response_json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse SFU response JSON: {}", e))?;

    log::info!("Successfully acquired LiveKit token!");

    let room = response_json["url"]
        .as_str()
        .ok_or("No url returned")?
        .clone();
    let jwt = response_json["jwt"]
        .as_str()
        .ok_or("No jwt returned")?
        .clone();

    debug!("url: {room}, token: {jwt}");

    use livekit::prelude::*;

    let (room, mut room_events) = Room::connect(&room, &jwt, RoomOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    while let Some(event) = room_events.recv().await {
        match event {
            RoomEvent::TrackSubscribed {
                track,
                publication,
                participant,
            } => {
                debug!(
                    "track: {:?}, publication: {:?}, participant: {:?}",
                    track, publication, participant
                );
                // ...
            }
            _ => {}
        }
    }

    Ok(response_json)
}
