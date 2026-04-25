use log::debug;
use std::sync::Arc;

use crate::{AppState, TauriError, construct_url};
use reqwest::Client;
use shared::breadcrumbs::Breadcrumbs;
use tauri::{State, command};

const BREADCRUMBS_TYPE: &str = "org.opal-matrix.breadcrumbs";

async fn set_breadcrumbs(
    matrix_url: &String,
    user_id: &String,
    access_token: &String,
    breadcrumbs: Breadcrumbs,
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
        BREADCRUMBS_TYPE,
    ])?;

    let body = breadcrumbs;

    client
        .put(url)
        .bearer_auth(access_token)
        .json(&body)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
}

async fn get_breadcrumbs(
    matrix_url: &String,
    user_id: &String,
    access_token: &String,
) -> Result<Breadcrumbs, TauriError> {
    let client = Client::new();

    let url = construct_url(vec![
        matrix_url,
        "_matrix",
        "client",
        "v3",
        "user",
        user_id,
        "account_data",
        BREADCRUMBS_TYPE,
    ])?;

    let response = client.get(url).bearer_auth(access_token).send().await?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Breadcrumbs::default());
    }

    let breadcrumbs = response.json::<Breadcrumbs>().await?;

    Ok(breadcrumbs)
}

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

    set_breadcrumbs(matrix_url, user_id, access_token, breadcrumbs).await
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

    get_breadcrumbs(matrix_url, user_id, access_token).await
}
