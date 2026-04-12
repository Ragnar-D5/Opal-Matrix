use crate::TauriError;
use matrix_sdk_crypto::OlmMachine;
use matrix_sdk_sqlite::SqliteCryptoStore;

use ruma::{OwnedDeviceId, UserId};

pub async fn init_crypto_machine(
    user_id: String,
    device_id: String,
    db_passphrase: &str,
) -> Result<OlmMachine, TauriError> {
    let ruma_user = UserId::parse(user_id)?;
    let ruma_device: OwnedDeviceId = device_id.into();

    let store = SqliteCryptoStore::open("./app_data/crypto.db", Some(db_passphrase)).await?;

    let machine = OlmMachine::with_store(&ruma_user, &ruma_device, store, None).await?;

    return Ok(machine);
}
