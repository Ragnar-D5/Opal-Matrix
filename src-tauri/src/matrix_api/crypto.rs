use log::warn;
use ruma::RoomId;
use ruma::api::client::backup::EncryptedSessionData;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

use crate::AppState;
use crate::authentication::get_account_data;
use bytes::Bytes;

use crate::APP_NAME;
use crate::TauriError;
use crate::construct_url;
use http::Response as HttpResponse;
use matrix_sdk_crypto::olm::ExportedRoomKey;
use matrix_sdk_crypto::store::types::BackupDecryptionKey;
use matrix_sdk_crypto::{
    DecryptionSettings, EncryptionSyncChanges, OlmMachine, types::requests::AnyOutgoingRequest,
};
use matrix_sdk_sqlite::SqliteCryptoStore;
use tauri_plugin_http::reqwest::{self, Client};

use log::{error, info};

use ruma::OwnedRoomId;
use ruma::api::client::sync::sync_events::v3::Response as SyncResponse;
use ruma::{OwnedDeviceId, UserId};

use keyring::Entry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::json;

const LAST_USER_KEY: &str = "__last_active_user__";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StoredSession {
    pub user_id: String,
    pub device_id: String,

    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,

    pub next_batch: Option<String>,

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

pub async fn init_crypto_machine(
    path: PathBuf,
    user_id: String,
    device_id: String,
    db_passphrase: String,
) -> Result<OlmMachine, TauriError> {
    let ruma_user = UserId::parse(user_id)?;
    let ruma_device: OwnedDeviceId = device_id.clone().into();

    let path = path.join(format!("crypto_store_{}", device_id));

    let store = SqliteCryptoStore::open(path.clone(), Some(&db_passphrase)).await?;

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

            info!("Generated and stored new passphrase for user {}", user_id);

            Ok(new_passphrase)
        }
        Err(e) => Err(format!("Keyring error: {}", e).into()),
    }
}

pub async fn process_message(
    state: &AppState,
    room_id: &String,
    raw_message: Value,
) -> Result<Value, TauriError> {
    let event_id = raw_message
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing event_id in message")?
        .to_string();
    let sender = raw_message
        .get("sender")
        .and_then(|v| v.as_str())
        .ok_or("Missing sender in message")?
        .to_string();
    let timestamp = raw_message
        .get("origin_server_ts")
        .and_then(|v| v.as_i64())
        .ok_or("Missing origin_server_ts in message")?;

    let olm_machine = {
        let guard = state.crypto_machine.lock().await;
        guard.as_ref().ok_or("Olm machine not initialized")?.clone()
    };
    let access_token = state.check_token().await?;
    let matrix_url = {
        let guard = state.matrix_url.read().await;
        guard.clone().ok_or("Matrix URL not set")?
    };

    let decryption_settings = DecryptionSettings {
        sender_device_trust_requirement: matrix_sdk_crypto::TrustRequirement::Untrusted,
    };

    handle_outgoing_requests(&olm_machine, &access_token, &matrix_url).await?;

    let event: Raw<EncryptedEvent> = Raw::from_json_string(raw_message.to_string())?;
    match olm_machine
        .decrypt_room_event(
            &event,
            &RoomId::parse(room_id.clone())?,
            &decryption_settings,
        )
        .await
    {
        Ok(res) => {
            let mut decrypted_val: Value = serde_json::from_str(res.event.into_json().get())?;

            if let Some(obj) = decrypted_val.as_object_mut() {
                obj.insert("event_id".to_string(), Value::String(event_id));
                obj.insert("sender".to_string(), Value::String(sender));
                obj.insert(
                    "origin_server_ts".to_string(),
                    Value::Number(timestamp.into()),
                );
            }

            return Ok(decrypted_val);
        }
        Err(e) => {
            warn!("Failed to decrypt event: {}", e);

            return Ok(Value::from_str(&raw_message.to_string())?);
        }
    };
}

use matrix_sdk_crypto::types::events::room::encrypted::EncryptedEvent;
use ruma::serde::Raw;

pub async fn process_sync_response(
    olm_machine: &OlmMachine,
    mut sync_res: SyncResponse,
    token: &String,
    matrix_url: &String,
) -> Result<SyncResponse, TauriError> {
    handle_outgoing_requests(olm_machine, token, matrix_url).await?;

    let decryption_settings = DecryptionSettings {
        sender_device_trust_requirement: matrix_sdk_crypto::TrustRequirement::Untrusted,
    };

    let sync_changes = EncryptionSyncChanges {
        to_device_events: sync_res.clone().to_device.events,
        changed_devices: &sync_res.device_lists,
        one_time_keys_counts: &sync_res.device_one_time_keys_count,
        unused_fallback_keys: sync_res.device_unused_fallback_key_types.as_deref(),
        next_batch_token: Some(sync_res.next_batch.clone()),
    };

    olm_machine
        .receive_sync_changes(sync_changes, &decryption_settings)
        .await?;

    handle_outgoing_requests(olm_machine, token, matrix_url).await?;

    for (room_id, joined_room) in sync_res.rooms.join.iter_mut() {
        let mut new_timeline = Vec::with_capacity(joined_room.timeline.events.len());

        for raw_event in joined_room.timeline.events.drain(..) {
            let mut replaced = false;

            if let Ok(val) = raw_event.deserialize_as::<serde_json::Value>() {
                if val.get("type").and_then(|t| t.as_str()) == Some("m.room.encrypted") {
                    let raw_json_value = raw_event.json().to_owned();
                    let encrypted_raw: Raw<EncryptedEvent> = Raw::from_json(raw_json_value);

                    match olm_machine
                        .decrypt_room_event(&encrypted_raw, &room_id, &decryption_settings)
                        .await
                    {
                        Ok(decrypted_result) => {
                            new_timeline.push(decrypted_result.event.cast());
                            replaced = true;
                        }
                        Err(e) => {
                            warn!("Failed to decrypt event in state: {}", e);

                            // Push original message without any modification
                            let owned = raw_event.json().to_owned();
                            let fallback_json = owned.get();

                            if let Ok(raw_fallback) = serde_json::from_str(fallback_json) {
                                new_timeline.push(raw_fallback);
                                replaced = true;
                            }
                        }
                    }
                }

                if !replaced {
                    new_timeline.push(raw_event);
                }
            }
        }

        joined_room.timeline.events = new_timeline;
    }

    return Ok(sync_res);
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

// #[derive(Deserialize, Debug)]
// enum Algorithm {
//     #[serde(rename = "m.megolm_backup.v1.curve25519-aes-sha2")]
//     MegolmV1AesSha2,
// }

// #[derive(Deserialize, Debug)]
// struct AuthData {
//     public_key: String,
//     signatures: BTreeMap<String, BTreeMap<String, String>>,

//     // To be sure
//     #[serde(flatten)]
//     extra: BTreeMap<String, Value>,
// }

#[derive(Deserialize, Debug)]
struct BackupInfoResponse {
    // algorithm: Algorithm,
    // auth_data: AuthData,
    // count: u64,
    // etag: String,
    version: String,
}
#[derive(Deserialize, Debug)]
struct KeyBackupData {
    // first_message_index: u64,
    // forwarded_count: u64,
    // is_verified: bool,
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

        version = json_res.version;
    } else {
        return Err(format!("Web request failed: {}", res.status()).into());
    }

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
    aes::Aes256,
    aes::cipher::{KeyInit, KeyIvInit, StreamCipher}, // KeyInit/KeyIvInit give us new_from_slice(s)
    base64::{Engine, engine::general_purpose::STANDARD as b64},
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

    let decoded_base58 = bs58::decode(clean_key).into_vec()?;
    let ssss_key = match decoded_base58.len() {
        35 => &decoded_base58[2..34], // Matrix keys have a 2-byte prefix and 1-byte suffix
        32 => &decoded_base58[..],
        _ => return Err("Invalid Matrix recovery key length".into()),
    };

    let ciphertext = b64.decode(ciphertext_b64)?;
    let iv = b64.decode(iv_b64)?;
    let expected_mac = b64.decode(mac_b64)?;

    if iv.len() != 16 {
        return Err("IV must be exactly 16 bytes".into());
    }

    let mut okm = [0u8; 64];
    Hkdf::<Sha256>::new(None, ssss_key)
        .expand(event_type.as_bytes(), &mut okm)
        .map_err(|_| "HKDF expansion failed")?;

    let (aes_key, hmac_key) = okm.split_at(32);

    let mut mac_verifier = Hmac::<Sha256>::new_from_slice(hmac_key)?;
    mac_verifier.update(&ciphertext);
    mac_verifier
        .verify_slice(&expected_mac)
        .map_err(|_| "MAC verification failed (Integrity check error)")?;

    let mut cipher = Ctr64BE::<Aes256>::new_from_slices(aes_key, &iv)
        .map_err(|_| "Invalid AES key or IV length")?;
    let mut plaintext = ciphertext.clone();
    cipher.apply_keystream(&mut plaintext);

    Ok(String::from_utf8(plaintext)?)
}
