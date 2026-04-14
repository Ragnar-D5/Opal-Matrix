use matrix_sdk_crypto::vodozemac::olm;
use ruma::api::client::backup::EncryptedSessionData;
use ruma::serde::Base64;
use std::collections::BTreeMap;
use std::str::FromStr;

use crate::authentication::get_account_data;
use bytes::Bytes;

use crate::construct_url;
use crate::TauriError;
use crate::APP_NAME;
use http::Response as HttpResponse;
use matrix_sdk_crypto::olm::ExportedRoomKey;
use matrix_sdk_crypto::store::types::BackupDecryptionKey;
use matrix_sdk_crypto::{
    types::requests::{AnyOutgoingRequest, OutgoingRequest},
    DecryptionSettings, EncryptionSyncChanges, OlmMachine,
};
use matrix_sdk_sqlite::SqliteCryptoStore;
use reqwest::Client;

use log::{error, info};

use reqwest::Response;
use ruma::api::client::sync::sync_events::v3::Response as SyncResponse;
use ruma::OwnedRoomId;
use ruma::{api::OutgoingRequestAppserviceExt, OwnedDeviceId, TransactionId, UserId};

use keyring::Entry;
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

    pub recovery_key: Option<String>,

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
        sender_device_trust_requirement: matrix_sdk_crypto::TrustRequirement::Untrusted,
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

#[derive(Deserialize, Debug)]
enum Algorithm {
    #[serde(rename = "m.megolm_backup.v1.curve25519-aes-sha2")]
    MegolmV1AesSha2,
}

#[derive(Deserialize, Debug)]
struct AuthData {
    public_key: String,
    signatures: BTreeMap<String, BTreeMap<String, String>>,

    // To be sure
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Deserialize, Debug)]
struct BackupInfoResponse {
    algorithm: Algorithm,
    auth_data: AuthData,
    count: u64,
    etag: String,
    version: String,
}

#[derive(Deserialize, Debug)]
struct BackupKeysRequest {
    version: String,
}

#[derive(Deserialize, Debug)]
struct SessionData {
    ciphertext: String,
    mac: String,
    ephemeral: String,
}

#[derive(Deserialize, Debug)]
struct KeyBackupData {
    first_message_index: u64,
    forwarded_count: u64,
    is_verified: bool,
    session_data: EncryptedSessionData,
}

#[derive(Deserialize, Debug)]
struct RoomKeyBackup {
    sessions: BTreeMap<String, KeyBackupData>,
}

#[derive(Deserialize, Debug)]
struct BackupKeysResponse {
    rooms: BTreeMap<String, RoomKeyBackup>,
}

pub async fn set_room_keys(
    olm_machine: &OlmMachine,
    matrix_url: &String,
    token: &String,
    recovery_key: &String,
) -> Result<(), TauriError> {
    // Get the key version
    let version;

    let client = Client::new();

    let url = construct_url(vec![
        matrix_url,
        "_matrix",
        "client",
        "v3",
        "room_keys",
        "version",
    ])?;

    let res = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if res.status().is_success() {
        let json_res: BackupInfoResponse = res
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;

        println!("Available backup versions: {:?}", json_res);

        version = json_res.version;
    } else {
        return Err(format!("Web request failed: {}", res.status()).into());
    }

    // Get the key from account data
    let res = get_account_data(
        token,
        matrix_url,
        &olm_machine.user_id().to_string(),
        &"m.secret_storage.default_key".to_string(),
    )
    .await?;

    let default_key_id = res["key"]
        .as_str()
        .ok_or("Missing default_key in account data")?;

    let res = get_account_data(
        token,
        matrix_url,
        &olm_machine.user_id().to_string(),
        &"m.megolm_backup.v1".to_string(),
    )
    .await?;

    let enc = &res["encrypted"][default_key_id];

    let ciphertext = enc["ciphertext"]
        .as_str()
        .ok_or("Missing ciphertext in encrypted key data")?;
    let mac = enc["mac"]
        .as_str()
        .ok_or("Missing mac in encrypted key data")?;
    let iv = enc["iv"]
        .as_str()
        .ok_or("Missing ephemeral in encrypted key data")?;

    let backup_private_key_b64 =
        decrypt_ssss_aes_hmac_sha2(recovery_key, "m.megolm_backup.v1", ciphertext, iv, mac)?;

    // Get the encrypted keys
    let url = construct_url(vec![
        matrix_url,
        "_matrix",
        "client",
        "v3",
        "room_keys",
        &format!("keys?version={}", version),
    ])?;

    let res = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if res.status().is_success() {
        let json_res: BackupKeysResponse = res
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;

        let decryption_key = BackupDecryptionKey::from_base64(&backup_private_key_b64)?;

        let backup_machine = olm_machine.backup_machine();

        backup_machine
            .save_decryption_key(Some(decryption_key.clone()), Some(version.clone()))
            .await?;

        let mut exported_keys = Vec::new();
        for (room_id, room_backup) in json_res.rooms {
            for (session_id, backup_data) in room_backup.sessions {
                match decryption_key.decrypt_session_data(backup_data.session_data) {
                    Ok(decrypted) => {
                        let exported_key = ExportedRoomKey {
                            algorithm: decrypted.algorithm,
                            sender_claimed_keys: decrypted.sender_claimed_keys,
                            forwarding_curve25519_key_chain: decrypted
                                .forwarding_curve25519_key_chain,
                            shared_history: decrypted.shared_history,

                            room_id: OwnedRoomId::from_str(&room_id)?,
                            session_id: session_id,
                            session_key: decrypted.session_key,
                            sender_key: decrypted.sender_key,
                        };

                        exported_keys.push(exported_key);
                    }
                    Err(e) => error!(
                        "Failed to decrypt backup key for room {}, session {}: {}",
                        room_id, session_id, e
                    ),
                }
            }
        }

        if !exported_keys.is_empty() {
            olm_machine
                .store()
                .import_room_keys(exported_keys, Some(version.as_str()), |_, _| {})
                .await?;
        } else {
            error!("No keys were decrypted successfully, nothing to import");
        }
    } else {
        return Err(format!("Web request failed: {}", res.status()).into());
    }

    handle_outgoing_requests(olm_machine, token, matrix_url).await?;

    Ok(())
}

use {
    aes::cipher::{KeyInit, KeyIvInit, StreamCipher}, // KeyInit/KeyIvInit give us new_from_slice(s)
    aes::Aes256,
    base64::{engine::general_purpose::STANDARD as b64, Engine},
    ctr::Ctr64BE,
    hkdf::Hkdf,
    hmac::{Hmac, Mac},
    sha2::Sha256,
};

pub fn decrypt_ssss_aes_hmac_sha2(
    recovery_key_base58: &str,
    event_type: &str, // e.g. "m.megolm_backup.v1"
    ciphertext_b64: &str,
    iv_b64: &str,
    mac_b64: &str,
) -> Result<String, TauriError> {
    let clean_key = recovery_key_base58.replace(" ", "");

    // 1) Decode Base58 Recovery Key
    let decoded_base58 = bs58::decode(clean_key).into_vec()?;
    let ssss_key = match decoded_base58.len() {
        35 => &decoded_base58[2..34], // Matrix keys have a 2-byte prefix and 1-byte suffix
        32 => &decoded_base58[..],
        _ => return Err("Invalid Matrix recovery key length".into()),
    };

    // Decode Base64 inputs
    let ciphertext = b64.decode(ciphertext_b64)?;
    let iv = b64.decode(iv_b64)?;
    let expected_mac = b64.decode(mac_b64)?;

    if iv.len() != 16 {
        return Err("IV must be exactly 16 bytes".into());
    }

    // 2) HKDF-SHA256 derivation
    let mut okm = [0u8; 64];
    Hkdf::<Sha256>::new(None, ssss_key)
        .expand(event_type.as_bytes(), &mut okm)
        .map_err(|_| "HKDF expansion failed")?;

    let (aes_key, hmac_key) = okm.split_at(32);

    // 3) Verify MAC
    let mut mac_verifier = Hmac::<Sha256>::new_from_slice(hmac_key)?;
    mac_verifier.update(&ciphertext);
    mac_verifier
        .verify_slice(&expected_mac)
        .map_err(|_| "MAC verification failed (Integrity check error)")?;

    let mut cipher = Ctr64BE::<Aes256>::new_from_slices(aes_key, &iv)
        .map_err(|_| "Invalid AES key or IV length")?;
    // 4) AES-CTR Decrypt
    let mut plaintext = ciphertext.clone();
    cipher.apply_keystream(&mut plaintext);

    // 5) Return UTF-8
    Ok(String::from_utf8(plaintext)?)
}
