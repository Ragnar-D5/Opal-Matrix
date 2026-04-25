use std::sync::Arc;

use crate::{
    AppState, TauriError, construct_url,
    matrix_api::{account_data::get_account_data, account_data::set_account_data},
};
use reqwest::Client;
use shared::account_data::Breadcrumbs;
use tauri::{State, command};

#[command]
pub async fn update_breadcrumbs(
    state: State<'_, Arc<AppState>>,
    breadcrumbs: Breadcrumbs,
) -> Result<(), TauriError> {
    let matrix_url = &state
        .matrix_url
        .read()
        .await
        .clone()
        .ok_or("Not logged in")?;
    let access_token = &state
        .token
        .read()
        .await
        .clone()
        .ok_or("Not logged in")?
        .access_token;
    let user_id = &state
        .client
        .read()
        .await
        .clone()
        .ok_or("Not logged in")?
        .user_id;

    set_account_data(matrix_url, user_id, access_token, breadcrumbs).await
}

#[command]
pub async fn fetch_breadcrumbs(state: State<'_, Arc<AppState>>) -> Result<Breadcrumbs, TauriError> {
    let matrix_url = &state
        .matrix_url
        .read()
        .await
        .clone()
        .ok_or("Not logged in")?;
    let access_token = &state
        .token
        .read()
        .await
        .clone()
        .ok_or("Not logged in")?
        .access_token;
    let user_id = &state
        .client
        .read()
        .await
        .clone()
        .ok_or("Not logged in")?
        .user_id;

    get_account_data(matrix_url, user_id, access_token).await
}
