use crate::{TauriError, construct_url};
use reqwest::Client;
use serde::{Serialize, de::DeserializeOwned};
use shared::account_data::AccountData;

pub async fn set_account_data<T: Serialize + AccountData>(
    matrix_url: &String,
    user_id: &String,
    access_token: &String,
    data: T,
) -> Result<(), TauriError> {
    let client = Client::new();

    let url = construct_url(vec![
        matrix_url,
        "_matrix",
        "client",
        "v3",
        "user",
        user_id,
        "account_data",
        T::DATA_KEY,
    ])?;

    client
        .put(url)
        .bearer_auth(access_token)
        .json(&data)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}

pub async fn get_account_data<T: DeserializeOwned + AccountData>(
    matrix_url: &String,
    user_id: &String,
    access_token: &String,
) -> Result<T, TauriError> {
    let client = Client::new();

    let url = construct_url(vec![
        matrix_url,
        "_matrix",
        "client",
        "v3",
        "user",
        user_id,
        "account_data",
        T::DATA_KEY,
    ])?;

    let response = client.get(url).bearer_auth(access_token).send().await?;

    if response.status().is_success() {
        let data = response.json::<T>().await?;
        Ok(data)
    } else {
        Err(format!(
            "Failed to fetch account data: HTTP {}: {}",
            response.status(),
            response.text().await?
        )
        .into())
    }
}
