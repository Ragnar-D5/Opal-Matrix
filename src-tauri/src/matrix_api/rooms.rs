use crate::{construct_url, TauriError};
use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize)]
struct JoinedRoomsResponse {
    joined_rooms: Vec<String>,
}

pub async fn get_rooms(
    access_token: String,
    matrix_url: String,
) -> Result<Vec<String>, TauriError> {
    let client = Client::new();

    let res = client
        .get(format!("{matrix_url}/_matrix/client/v3/joined_rooms"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    if res.status().is_success() {
        let json_res: JoinedRoomsResponse =
            res.json().await.map_err(|e| format!("Parse error: {e}"))?;

        Ok(json_res.joined_rooms)
    } else {
        Err(format!("Failed to fetch rooms: {}", res.status()).into())
    }
}
