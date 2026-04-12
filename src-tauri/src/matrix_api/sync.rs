use std::collections::HashMap;

use crate::{construct_url, TauriError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use matrix_sdk_crypto::EncryptionSyncChanges;
use ruma::api::{client::sync::sync_events::v3::Response as SyncResponse, IncomingResponse};

#[derive(Deserialize, Debug)]
pub struct MatrixEvent {
    pub content: Value,
    #[serde(rename = "type")]
    pub event_type: String,
}

#[derive(Deserialize, Debug)]
pub struct MatrixAccountData {
    pub events: Vec<MatrixEvent>,
}

#[derive(Deserialize, Debug)]
pub struct MatrixEphemeral {
    pub events: Vec<MatrixEvent>,
}

#[derive(Deserialize, Debug)]
pub struct UnsignedData {
    pub age: Option<i64>,
    pub membership: Option<String>,
    pub prev_content: Option<Value>,
    // pub redacted_because: Option<MatrixClientEventWithoutRoomID>,
    pub transaction_id: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct MatrixClientEventWithoutRoomID {
    pub content: Value,
    #[serde(rename = "type")]
    pub event_type: String,
    pub event_id: String,
    pub origin_server_ts: i64,
    pub sender: String,
    pub state_key: Option<String>,
    pub unsigned: Option<UnsignedData>,
}

#[derive(Deserialize, Debug)]
pub struct MatrixState {
    events: Vec<MatrixClientEventWithoutRoomID>,
}

#[derive(Deserialize, Debug)]
pub struct MatrixRoomSummary {
    #[serde(rename = "m.heroes")]
    pub heroes: Option<Vec<String>>,
    #[serde(rename = "m.invited_member_count")]
    pub invited_member_count: Option<u64>,
    #[serde(rename = "m.joined_member_count")]
    pub joined_member_count: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub struct MatrixTimeline {
    pub events: Vec<MatrixClientEventWithoutRoomID>,
    pub limited: Option<bool>,
    pub prev_batch: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct MatrixUnreadNotificationCounts {
    pub highlight_count: Option<u64>,
    pub notification_count: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub struct MatrixThreadNotificationCounts {
    pub highlight_count: Option<u64>,
    pub notification_count: Option<u64>,
}

#[derive(Deserialize, Debug)]
pub struct MatrixJoinedRoom {
    pub account_data: Option<MatrixAccountData>,
    pub ephemeral: Option<MatrixEphemeral>,
    pub state: Option<MatrixState>,
    pub state_after: Option<MatrixState>,
    pub summary: Option<MatrixRoomSummary>,
    pub timeline: Option<MatrixTimeline>,

    pub unread_notifications: Option<MatrixUnreadNotificationCounts>,

    pub unread_thread_notifications: Option<HashMap<String, MatrixThreadNotificationCounts>>,

    pub to_device: Option<Value>,
}

#[derive(Deserialize, Debug)]
pub struct MatrixRooms {
    pub join: HashMap<String, MatrixJoinedRoom>,
}

#[derive(Deserialize, Debug)]
pub struct MatrixPresence {
    pub events: Vec<MatrixEvent>,
}

#[derive(Deserialize, Debug)]
pub struct MatrixSyncResponse {
    // pub account_data: MatrixAccountData,
    next_batch: String,
    presence: MatrixPresence,
    pub rooms: MatrixRooms,
}

pub async fn matrix_sync(
    access_token: String,
    matrix_url: String,
) -> Result<SyncResponse, TauriError> {
    let client = Client::new();

    let url = construct_url(vec![
        matrix_url,
        "_matrix".to_string(),
        "client".to_string(),
        "v3".to_string(),
        "sync".to_string(),
    ])?;

    let res = client
        .get(url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let status = res.status();
    let headers = res.headers().clone();
    let body_bytes = res
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    let mut builder = http::Response::builder().status(status);

    for (key, value) in headers.iter() {
        builder = builder.header(key, value);
    }

    let http_response = builder
        .body(body_bytes.to_vec())
        .map_err(|e| format!("Failed to build HTTP response: {e}"))?;

    match SyncResponse::try_from_http_response(http_response) {
        Ok(sync_response) => Ok(sync_response),
        Err(e) => Err(format!("Failed to parse sync response: {e}").into()),
    }
}
