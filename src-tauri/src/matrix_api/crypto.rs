use crate::TauriError;
use crate::APP_NAME;
use matrix_sdk_crypto::{DecryptionSettings, EncryptionSyncChanges, OlmMachine};
use matrix_sdk_sqlite::SqliteCryptoStore;

use log::info;

use ruma::api::client::sync::sync_events::v3::Response as SyncResponse;

use keyring::Entry;
use ruma::{OwnedDeviceId, UserId};
use serde::{Deserialize, Serialize};

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

pub async fn process_sync_response(
    olm_machine: &OlmMachine,
    sync_res: SyncResponse,
) -> Result<(), TauriError> {
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

    let outgoing_requests = olm_machine.outgoing_requests().await?;

    // println!("{:?}", outgoing_requests);

    return Ok(());
}
