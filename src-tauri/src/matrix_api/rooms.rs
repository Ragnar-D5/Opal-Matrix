use ruma::OwnedRoomId;
use ruma::api::{IncomingResponse, OutgoingRequest};
use std::borrow::Cow;
use std::{str::FromStr, sync::Arc};
use tauri_plugin_http::reqwest::{self, Client};

use ruma::api::{
    auth_scheme::SendAccessToken,
    client::membership::joined_members::v3::{
        Request as JoinedMembersRequest, Response as JoinedMembersResponse,
    },
};

use crate::{
    AppState, TauriError, reqwest_response_to_http_response,
    state::HomeServerInfo,
    storage::members::{MemberRow, MembershipState},
};
use tauri::{State, command};

pub async fn get_members_api(
    server_info: &HomeServerInfo,
    access_token: String,
    room_id: String,
) -> Result<Vec<MemberRow>, TauriError> {
    let req = JoinedMembersRequest::new(OwnedRoomId::from_str(room_id.as_str())?)
        .try_into_http_request::<Vec<u8>>(
            server_info.base_url.as_str(),
            SendAccessToken::Always(access_token.as_str()),
            Cow::Owned(server_info.supported_versions.clone()),
        )?;

    let http_req = reqwest::Request::try_from(req)?;

    let res = reqwest_response_to_http_response(Client::new().execute(http_req).await?).await?;

    let mut members = Vec::new();
    let joined_members = JoinedMembersResponse::try_from_http_response(res)?.joined;

    for (user_id, member_info) in joined_members {
        members.push(MemberRow {
            room_id: room_id.clone(),
            user_id: user_id.to_string(),
            display_name: member_info.display_name.clone(),
            avatar_url: member_info.avatar_url.clone().map(|m| m.to_string()),
            membership: MembershipState::Join,
        });
    }

    return Ok(members);
}

/// Sends a read marker to the Matrix server for a specific room and event, indicating that the user has read up to that event.
///
/// Example usage in a leptos frontend:
/// ```rust
/// use crate::matrix_api::rooms::send_read_marker;
/// use leptos::prelude::*;
///
/// let payload = SendMarkerPayload {
///     room_id: "!roomid:example.com".to_string(),
///     event_id: "$eventid:example.com".to_string(),
/// };
///
/// invoke("send_read_marker", serde_wasm_bindgen::to_value(&payload)?).await?;
/// ```
#[command(rename_all = "snake_case")]
pub async fn send_read_marker(
    state: State<'_, Arc<AppState>>,
    room_id: String,
    event_id: String,
) -> Result<(), TauriError> {
    let (token, url) = state
        .get_api_with_url(vec![
            "_matrix",
            "client",
            "v3",
            "rooms",
            &room_id,
            "read_markers",
        ])
        .await?;

    Client::new()
        .post(url)
        .bearer_auth(token)
        .json(&serde_json::json!({
            "m.fully_read": event_id,
            "m.read": event_id,
        }))
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}
