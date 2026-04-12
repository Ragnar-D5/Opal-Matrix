use bytes::Bytes;

use crate::TauriError;
use crate::APP_NAME;
use http::Response as HttpResponse;
use matrix_sdk_crypto::types::requests::AnyOutgoingRequest;
use matrix_sdk_crypto::types::requests::OutgoingRequest;
use matrix_sdk_crypto::{DecryptionSettings, EncryptionSyncChanges, OlmMachine};
use matrix_sdk_sqlite::SqliteCryptoStore;

use log::{error, info};

use reqwest::Response;
use ruma::api::client::sync::sync_events::v3::Response as SyncResponse;
use ruma::api::OutgoingRequestAppserviceExt;
use ruma::TransactionId;

use keyring::Entry;
use ruma::{OwnedDeviceId, UserId};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;

const LAST_USER_KEY: &str = "__last_active_user__";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredSession {
    pub user_id: String,
    pub device_id: String,

    pub access_token: String,
    pub refresh_token: Option<String>,
    pub homeserver_url: String,
}

pub async fn get_last_active_session() -> Result<Option<StoredSession>, TauriError> {
    let entry = Entry::new(APP_NAME, LAST_USER_KEY)?;

    match entry.get_password() {
        Ok(user_id) => get_session(user_id).await,
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Keyring error: {}", e).into()),
    }
}

pub async fn get_session(user_id: String) -> Result<Option<StoredSession>, TauriError> {
    let entry_key = format!("{}:session", user_id);
    let entry = Entry::new(APP_NAME, &entry_key)?;

    match entry.get_password() {
        Ok(session_json) => {
            let session: StoredSession = serde_json::from_str(&session_json)
                .map_err(|e| format!("Failed to parse session data: {}", e))?;
            Ok(Some(session))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => {
            return Err(format!("Keyring error: {}", e).into());
        }
    }
}

pub async fn save_session(session: &StoredSession) -> Result<(), TauriError> {
    let entry_key = format!("{}:session", session.user_id);
    let entry = Entry::new(APP_NAME, &entry_key)?;

    let session_json = serde_json::to_string(session)
        .map_err(|e| format!("Failed to serialize session data: {}", e))?;

    entry.set_password(&session_json)?;

    let last_user_entry = Entry::new(APP_NAME, LAST_USER_KEY)?;
    last_user_entry.set_password(&session.user_id)?;

    Ok(())
}

pub async fn get_device_id(user_id: String) -> Result<String, TauriError> {
    let entry = Entry::new(APP_NAME, &user_id)?;

    match entry.get_password() {
        Ok(device_id) => Ok(device_id),
        Err(keyring::Error::NoEntry) => {
            Err(format!("No device ID found for user {}", user_id).into())
        }
        Err(e) => Err(format!("Keyring error: {}", e).into()),
    }
}

pub async fn init_crypto_machine(
    user_id: String,
    device_id: String,
    db_passphrase: String,
) -> Result<OlmMachine, TauriError> {
    let ruma_user = UserId::parse(user_id)?;
    let ruma_device: OwnedDeviceId = device_id.clone().into();

    let db_path = format!("./app_data/crypto_{}.db", device_id);

    let store = SqliteCryptoStore::open(db_path, Some(&db_passphrase)).await?;

    let machine = OlmMachine::with_store(&ruma_user, &ruma_device, store, None).await?;

    return Ok(machine);
}

pub async fn get_or_create_passphrase(user_id: String) -> Result<String, TauriError> {
    let entry = Entry::new(APP_NAME, &format!("passphrase:{}", user_id))?;

    match entry.get_password() {
        Ok(passphrase) => Ok(passphrase),
        Err(keyring::Error::NoEntry) => {
            info!(
                "No existing passphrase found for user {}, generating a new one",
                user_id
            );

            let mut key_bytes = [0u8; 32];

            getrandom::fill(&mut key_bytes)?;

            let new_passphrase = hex::encode(key_bytes);

            entry.set_password(&new_passphrase)?;

            info!("New passphrase: {}", new_passphrase);

            Ok(new_passphrase)
        }
        Err(e) => Err(format!("Keyring error: {}", e).into()),
    }
}

#[derive(Serialize, Debug)]
pub struct RoomData {
    pub room_id: String,
    pub name: Option<String>,
    pub timeline: Vec<EventData>,
}

use matrix_sdk_crypto::types::events::room::encrypted::EncryptedEvent;
use ruma::api::client::sync::sync_events::v3::State;
use ruma::serde::Raw;

#[derive(Serialize, Debug)]
pub struct EventData {
    pub event_id: String,
    pub sender: String,
    pub origin_server_ts: u64,
    pub content: serde_json::Value, // The decrypted or cleartext content
    pub event_type: String,
}

pub async fn process_sync_response(
    olm_machine: &OlmMachine,
    sync_res: SyncResponse,
    token: &String,
    matrix_url: &String,
) -> Result<Vec<RoomData>, TauriError> {
    handle_outgoing_requests(olm_machine, token, matrix_url).await?;

    let decryption_settings = DecryptionSettings {
        sender_device_trust_requirement: matrix_sdk_crypto::TrustRequirement::CrossSigned,
    };

    let sync_changes = EncryptionSyncChanges {
        to_device_events: sync_res.to_device.events,
        changed_devices: &sync_res.device_lists,
        one_time_keys_counts: &sync_res.device_one_time_keys_count,
        unused_fallback_keys: sync_res.device_unused_fallback_key_types.as_deref(),
        next_batch_token: Some(sync_res.next_batch),
    };

    olm_machine
        .receive_sync_changes(sync_changes, &decryption_settings)
        .await?;

    handle_outgoing_requests(olm_machine, token, matrix_url).await?;

    let mut processed_rooms = Vec::new();

    for (room_id, joined_room) in sync_res.rooms.join {
        let mut room_name = room_id.to_string();

        let state_events = match joined_room.state {
            State::Before(s) => s.events,
            State::After(s) => s.events,
            _ => Vec::new(),
        };

        // Parse state for Room Name
        for raw_event in state_events {
            // raw_event is already a Raw<AnyStateEvent>, call deserialize_as directly
            if let Ok(val) = raw_event.deserialize_as::<serde_json::Value>() {
                if val["type"] == "m.room.name" {
                    if let Some(name) = val["content"]["name"].as_str() {
                        room_name = name.to_string();
                    }
                }
            }
        }

        let mut messages = Vec::new();

        for timeline_event in joined_room.timeline.events {
            if let Ok(event_json) = timeline_event.deserialize_as::<Value>() {
                let event_id = event_json["event_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let sender = event_json["sender"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let ts = event_json["origin_server_ts"].as_u64().unwrap_or_default();

                let mut content = event_json["content"].clone();
                let mut is_encrypted = false;

                if event_json["type"] == "m.room.encrypted" {
                    is_encrypted = true;

                    let raw_json_value = timeline_event.json().to_owned();
                    let encrypted_raw: Raw<EncryptedEvent> = Raw::from_json(raw_json_value);

                    match olm_machine
                        .decrypt_room_event(&encrypted_raw, &room_id, &decryption_settings)
                        .await
                    {
                        Ok(decrypted_result) => {
                            // decrypted_result.event is Raw<AnySyncTimelineEvent>
                            if let Ok(dec_json) =
                                decrypted_result.event.deserialize_as::<serde_json::Value>()
                            {
                                content = dec_json["content"].clone();
                            }
                        }
                        Err(e) => {
                            content = serde_json::json!({ "body": format!("** Decryption Error: {} **", e) });
                        }
                    }
                }

                if event_json["type"] == "m.room.message" || is_encrypted {
                    messages.push(EventData {
                        event_id,
                        sender,
                        origin_server_ts: ts,
                        content,
                        event_type: event_json["type"].as_str().unwrap_or_default().to_string(),
                    });
                }
            }
        }

        processed_rooms.push(RoomData {
            room_id: room_id.to_string(),
            name: Some(room_name),
            timeline: messages,
        });
    }

    handle_outgoing_requests(olm_machine, token, matrix_url).await?;

    return Ok(processed_rooms);
}

async fn handle_outgoing_requests(
    olm_machine: &OlmMachine,
    token: &String,
    matrix_url: &String,
) -> Result<(), TauriError> {
    let requests = olm_machine.outgoing_requests().await?;

    let client = reqwest::Client::new();

    for request in requests {
        let id = request.request_id();

        use ruma::api::IncomingResponse;
        match request.request() {
            AnyOutgoingRequest::KeysUpload(inner) => {
                let url = format!("{}/_matrix/client/v3/keys/upload", matrix_url);

                let body = json!({
                    "device_keys": inner.device_keys,
                    "one_time_keys": inner.one_time_keys,
                    "fallback_keys": inner.fallback_keys,
                });

                let http_res = send_post(&client, url, body, &token).await?;

                let matrix_res =
                    ruma::api::client::keys::upload_keys::v3::Response::try_from_http_response(
                        http_res,
                    )?;

                olm_machine.mark_request_as_sent(id, &matrix_res).await?;
            }
            AnyOutgoingRequest::KeysClaim(inner) => {
                let url = format!("{}/_matrix/client/v3/keys/claim", matrix_url);

                let body = json!({
                    "one_time_keys": inner.one_time_keys,
                });

                let http_res = self::send_post(&client, url, body, &token).await?;

                let matrix_res =
                    ruma::api::client::keys::claim_keys::v3::Response::try_from_http_response(
                        http_res,
                    )?;

                olm_machine.mark_request_as_sent(id, &matrix_res).await?;
            }
            AnyOutgoingRequest::KeysQuery(inner) => {
                let url = format!("{}/_matrix/client/v3/keys/query", matrix_url);

                let body = json!({
                    "device_keys": inner.device_keys,
                });

                let http_res = self::send_post(&client, url, body, &token).await?;

                let matrix_res =
                    ruma::api::client::keys::get_keys::v3::Response::try_from_http_response(
                        http_res,
                    )?;

                olm_machine.mark_request_as_sent(id, &matrix_res).await?;
            }
            AnyOutgoingRequest::RoomMessage(inner) => {
                let url = format!(
                    "{}/_matrix/client/v3/rooms/{}/send/m.room.encrypted/{}",
                    matrix_url, inner.room_id, inner.txn_id
                );

                let body = serde_json::to_value(&inner.content)?;

                let http_res = send_put(&client, url, body, &token).await?;

                let matrix_res =
                    ruma::api::client::message::send_message_event::v3::Response::try_from_http_response(
                        http_res,
                )?;

                olm_machine.mark_request_as_sent(id, &matrix_res).await?;
            }
            AnyOutgoingRequest::SignatureUpload(inner) => {
                let url = format!("{}/_matrix/client/v3/keys/signatures/upload", matrix_url);

                let body = json!({
                    "signatures": inner.signed_keys,
                });

                let http_res = self::send_post(&client, url, body, &token).await?;

                let matrix_res =
                    ruma::api::client::keys::upload_signatures::v3::Response::try_from_http_response(
                        http_res,
                    )?;

                olm_machine.mark_request_as_sent(id, &matrix_res).await?;
            }
            AnyOutgoingRequest::ToDeviceRequest(inner) => {
                let url = format!(
                    "{}/_matrix/client/v3/sendToDevice/{}/{}",
                    matrix_url, inner.event_type, inner.txn_id
                );

                let body = json!({
                    "messages": inner.messages,
                });

                let http_res = self::send_put(&client, url, body, &token).await?;

                let matrix_res =
                    ruma::api::client::to_device::send_event_to_device::v3::Response::try_from_http_response(http_res)?;

                olm_machine.mark_request_as_sent(id, &matrix_res).await?;
            }
        };
    }

    info!("All outgoing requests processed");

    Ok(())
}

async fn send_post(
    client: &reqwest::Client,
    url: String,
    body: Value,
    token: &String,
) -> Result<HttpResponse<Bytes>, TauriError> {
    let res = client
        .post(url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;

    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.bytes().await?;

    let mut builder = HttpResponse::builder().status(status);

    for (key, value) in headers.iter() {
        builder = builder.header(key, value);
    }

    builder.body(bytes).map_err(|e| e.to_string().into())
}

async fn send_put(
    client: &reqwest::Client,
    url: String,
    body: Value,
    token: &String,
) -> Result<HttpResponse<Bytes>, TauriError> {
    let res = client
        .put(url) // Using PUT instead of POST
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;

    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.bytes().await?;

    let mut builder = HttpResponse::builder().status(status);

    for (key, value) in headers.iter() {
        builder = builder.header(key, value);
    }

    builder.body(bytes).map_err(|e| e.to_string().into())
}
