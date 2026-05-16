use std::sync::Arc;

use crate::{construct_url, AppState, TauriError};
use serde::{de::DeserializeOwned, Serialize};
use shared::account_data::{AccountData, AccountDataPayload, AccountDataType};
use tauri::{command, State};
use tauri_plugin_http::reqwest::Client;

/// Generic function to set account data for a given type T that implements the `AccountData` trait.
async fn set_account_data_api<T: Serialize + AccountData>(
    matrix_url: &String,
    user_id: &String,
    access_token: &String,
    data: T,
) -> Result<(), TauriError> {
    let client = Client::new();

    let url = construct_url(vec![
        matrix_url.as_str(),
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

/// Generic function to get account data for a given type T that implements the `AccountData` trait.
async fn get_account_data_api<T: DeserializeOwned + Serialize + AccountData + Default>(
    matrix_url: &String,
    user_id: &String,
    access_token: &String,
) -> Result<T, TauriError> {
    let client = Client::new();

    let url = construct_url(vec![
        matrix_url.as_str(),
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

/// Command to set account data, which can handle multiple types of account data based on the payload.
///
/// Example usage from a leptos frontend:
/// ```rust
/// use shared::account_data::{AccountDataPayload, Breadcrumbs};
/// use leptos::prelude::*;
///
/// let payload = AccountDataPayload::Breadcrumbs(Breadcrumbs {
///    room_ids: vec!["!roomid:example.com".to_string()],
/// });
///
/// invoke("set_account_data", serde_wasm_bindgen::to_value(&payload)?).await?;
/// ```
#[command]
pub async fn set_account_data(
    state: State<'_, Arc<AppState>>,
    payload: AccountDataPayload,
) -> Result<(), TauriError> {
    let (matrix_url, access_token, user_id) = {
        let m = state.home_server_info.read().await;
        let t = state.token.read().await;
        let c = state.client.read().await;

        let url = m.as_ref().ok_or("Not logged in")?.clone();
        let token = t.as_ref().ok_or("Not logged in")?.access_token.clone();
        let id = c.as_ref().ok_or("Not logged in")?.user_id.clone();
        (url.base_url, token, id)
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

/// Command to get account data. The `data_type` parameter specifies which type of account data to fetch (e.g., "breadcrumbs" or "server_order").
///
/// Example usage from a leptos frontend:
/// ```rust
/// use shared::account_data::{AccountDataPayload, AccountDataType, GetAccountDataArgs};
/// use leptos::prelude::*;
///
/// let payload = GetAccountDataArgs {
///     data_type: AccountDataType::Breadcrumbs,
/// };
///
/// let data = invoke("get_account_data", serde_wasm_bindgen::to_value(&payload)?).await?;
/// ```
#[command(rename_all = "snake_case")]
pub async fn get_account_data(
    state: State<'_, Arc<AppState>>,
    data_type: AccountDataType,
) -> Result<AccountDataPayload, TauriError> {
    let (matrix_url, access_token, user_id) = {
        let m = state.home_server_info.read().await;
        let t = state.token.read().await;
        let c = state.client.read().await;

        let url = m.as_ref().ok_or("Not logged in")?.clone();
        let token = t.as_ref().ok_or("Not logged in")?.access_token.clone();
        let id = c.as_ref().ok_or("Not logged in")?.user_id.clone();
        (url.base_url, token, id)
    };

    match data_type {
        AccountDataType::Breadcrumbs => {
            let data = get_account_data_api::<shared::account_data::Breadcrumbs>(
                &matrix_url,
                &user_id,
                &access_token,
            )
            .await?;
            Ok(AccountDataPayload::Breadcrumbs(data))
        }
        AccountDataType::ServerOrder => {
            let data = get_account_data_api::<shared::account_data::ServerOrder>(
                &matrix_url,
                &user_id,
                &access_token,
            )
            .await?;
            Ok(AccountDataPayload::ServerOrder(data))
        }
    }
}

/// Convenience command to directly get breadcrumbs without needing to specify the data type.
///
/// Example usage from a leptos frontend:
/// ```rust
/// use leptos::prelude::*;
///
/// let breadcrumbs = invoke("get_breadcrumbs", ()).await?;
/// ```
#[command]
pub async fn get_breadcrumbs(
    state: State<'_, Arc<AppState>>,
) -> Result<shared::account_data::Breadcrumbs, TauriError> {
    match get_account_data(state, AccountDataType::Breadcrumbs).await? {
        AccountDataPayload::Breadcrumbs(data) => Ok(data),
        _ => Err("Unexpected account data type".into()),
    }
}

/// Convenience command to directly get server order without needing to specify the data type.
///
/// Example usage from a leptos frontend:
/// ```rust
/// use leptos::prelude::*;
///
/// let server_order = invoke("get_server_order", ()).await?;
/// ```
#[command]
pub async fn get_server_order(
    state: State<'_, Arc<AppState>>,
) -> Result<shared::account_data::ServerOrder, TauriError> {
    match get_account_data(state, AccountDataType::ServerOrder).await? {
        AccountDataPayload::ServerOrder(data) => Ok(data),
        _ => Err("Unexpected account data type".into()),
    }
}
