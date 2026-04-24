use crate::{TauriError, construct_url};
use reqwest::Client;
use shared::breadcrumbs::Breadcrumbs;

pub async fn set_breadcrumbs(
    client: &Client,
    matrix_url: &String,
    user_id: &String,
    access_token: &str,
    breadcrumbs: Breadcrumbs,
) -> Result<(), TauriError> {
    let url = construct_url(vec![
        matrix_url,
        "_matrix",
        "client",
        "v3",
        "user",
        user_id,
        "account_data",
    ])?;

    let body = serde_json::json!({
        "crumbs": breadcrumbs,
    });

    // client
    //     .put(&url)
    //     .bearer_auth(access_token)
    //     .json(&body)
    //     .send()
    //     .await?
    //     .error_for_status()?;
    //
    Ok(())
}
