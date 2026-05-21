use serde_json::Value;
use tauri_plugin_http::reqwest::Client;

use crate::{TauriError, construct_url};

pub async fn _get_account_data(
    token: &str,
    matrix_url: &str,
    user_id: &str,
    data_type: &str,
) -> Result<Value, TauriError> {
    let client = Client::new();

    let url = construct_url(vec![
        matrix_url,
        "_matrix",
        "client",
        "v3",
        "user",
        user_id,
        "account_data",
        data_type,
    ])?;

    let res = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if res.status().is_success() {
        let json_res: Value = res.json().await.map_err(|e| format!("Parse error: {e}"))?;

        Ok(json_res)
    } else {
        Err(format!("Web request failed: {}", res.status()).into())
    }
}
