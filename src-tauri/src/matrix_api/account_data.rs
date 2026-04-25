use std::sync::Arc;

use crate::{AppState, TauriError, construct_url};
use reqwest::Client;
use serde::{Serialize, de::DeserializeOwned};
use shared::account_data::{AccountData, AccountDataPayload};
use tauri::{State, command};

async fn set_account_data_api<T: Serialize + AccountData>(
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

async fn get_account_data_api<T: DeserializeOwned + Serialize + AccountData + Default>(
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
    } else if response.status().as_u16() == 404 {
        set_account_data_api(matrix_url, user_id, access_token, T::default()).await?;
        Ok(T::default())
    } else {
        Err(format!(
            "Failed to fetch account data: HTTP {}: {}",
            response.status(),
            response.text().await?
        )
        .into())
    }
}

#[command]
pub async fn set_account_data(
    state: State<'_, Arc<AppState>>,
    payload: AccountDataPayload,
) -> Result<(), TauriError> {
    let (matrix_url, access_token, user_id) = {
        let m = state.matrix_url.read().await;
        let t = state.token.read().await;
        let c = state.client.read().await;

        let url = m.as_ref().ok_or("Not logged in")?.clone();
        let token = t.as_ref().ok_or("Not logged in")?.access_token.clone();
        let id = c.as_ref().ok_or("Not logged in")?.user_id.clone();
        (url, token, id)
    };

    match payload {
        AccountDataPayload::Breadcrumbs(data) => {
            set_account_data_api(&matrix_url, &user_id, &access_token, data).await?
        }
        AccountDataPayload::ServerOrder(data) => {
            set_account_data_api(&matrix_url, &user_id, &access_token, data).await?
        }
    }

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn get_account_data(
    state: State<'_, Arc<AppState>>,
    data_type: String,
) -> Result<AccountDataPayload, TauriError> {
    let (matrix_url, access_token, user_id) = {
        let m = state.matrix_url.read().await;
        let t = state.token.read().await;
        let c = state.client.read().await;

        let url = m.as_ref().ok_or("Not logged in")?.clone();
        let token = t.as_ref().ok_or("Not logged in")?.access_token.clone();
        let id = c.as_ref().ok_or("Not logged in")?.user_id.clone();
        (url, token, id)
    };

    match data_type.as_str() {
        "breadcrumbs" => {
            let data = get_account_data_api::<shared::account_data::Breadcrumbs>(
                &matrix_url,
                &user_id,
                &access_token,
            )
            .await?;
            Ok(AccountDataPayload::Breadcrumbs(data))
        }
        "server_order" => {
            let data = get_account_data_api::<shared::account_data::ServerOrder>(
                &matrix_url,
                &user_id,
                &access_token,
            )
            .await?;
            Ok(AccountDataPayload::ServerOrder(data))
        }
        _ => Err("Unknown account data type".into()),
    }
}

#[command]
pub async fn get_breadcrumbs(
    state: State<'_, Arc<AppState>>,
) -> Result<shared::account_data::Breadcrumbs, TauriError> {
    match get_account_data(state, "breadcrumbs".to_string()).await? {
        AccountDataPayload::Breadcrumbs(data) => Ok(data),
        _ => Err("Unexpected account data type".into()),
    }
}

#[command]
pub async fn get_server_order(
    state: State<'_, Arc<AppState>>,
) -> Result<shared::account_data::ServerOrder, TauriError> {
    match get_account_data(state, "server_order".to_string()).await? {
        AccountDataPayload::ServerOrder(data) => Ok(data),
        _ => Err("Unexpected account data type".into()),
    }
}
