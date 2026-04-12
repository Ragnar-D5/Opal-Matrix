use crate::TauriError;
use crate::APP_NAME;
use matrix_sdk_crypto::OlmMachine;
use matrix_sdk_sqlite::SqliteCryptoStore;

use keyring::Entry;
use ruma::{OwnedDeviceId, UserId};

pub async fn init_crypto_machine(
    user_id: String,
    device_id: String,
    db_passphrase: String,
) -> Result<OlmMachine, TauriError> {
    let ruma_user = UserId::parse(user_id)?;
    let ruma_device: OwnedDeviceId = device_id.into();

    let store = SqliteCryptoStore::open("./app_data/crypto.db", Some(&db_passphrase)).await?;

    let machine = OlmMachine::with_store(&ruma_user, &ruma_device, store, None).await?;

    return Ok(machine);
}

pub async fn get_or_create_passphrase(user_id: String) -> Result<String, TauriError> {
    let entry = Entry::new(APP_NAME, &user_id)?;

    match entry.get_password() {
        Ok(passphrase) => Ok(passphrase),
        Err(keyring::Error::NoEntry) => {
            let mut key_bytes = [0u8; 32];

            getrandom::fill(&mut key_bytes)?;

            let new_passphrase = hex::encode(key_bytes);

            entry.set_password(&new_passphrase)?;

            Ok(new_passphrase)
        }
        Err(e) => Err(format!("Keyring error: {}", e).into()),
    }
}
