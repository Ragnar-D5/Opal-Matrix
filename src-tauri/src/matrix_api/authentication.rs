use std::borrow::Cow;

use crate::{
    construct_url, reqwest_response_to_http_response,
    state::{ClientInfo, HomeServerInfo, RefreshToken, Token},
};
use log::error;
use ruma::api::{
    auth_scheme::SendAccessToken,
    client::{
        session::{
            login::v3::{LoginInfo, Password, Request as LoginRequest, Response as LoginResponse},
            refresh_token::v3::{Request as RefrefreshRequest, Response as RefreshResponse},
        },
        uiaa::UserIdentifier,
    },
    IncomingResponse, OutgoingRequest,
};
use serde_json::Value;
use shared::api::errors::LoginError;

use crate::TauriError;
use tauri_plugin_http::reqwest::{self, Client};

/// Logs in to the Matrix server using the provided username and password, returning the access token and refresh token (if supported by the server).
pub async fn matrix_login(
    server_info: HomeServerInfo,
    username: String,
    password: String,
) -> Result<(ClientInfo, Token), LoginError> {
    let client = Client::new();

    let mut req = LoginRequest::new(LoginInfo::Password(Password::new(
        UserIdentifier::UserIdOrLocalpart(username),
        password,
    )));

    req.device_id = None;
    req.initial_device_display_name = Some("Opal Matrix on Linux".to_string());
    req.refresh_token = true;

    let req = req
        .try_into_http_request::<Vec<u8>>(
            server_info.base_url.as_str(),
            SendAccessToken::None,
            Cow::Owned(server_info.supported_versions),
        )
        .map_err(|e| {
            error!("Failed to construct login request: {e}");
            LoginError::BackendError
        })?;

    let http_req = reqwest::Request::try_from(req).map_err(|_| LoginError::BackendError)?;

    let res = reqwest_response_to_http_response(client.execute(http_req).await.map_err(|e| {
        error!("Network error during login: {:?}", e);
        LoginError::NetworkError
    })?)
    .await
    .map_err(|e| {
        error!("Error converting response to http: {:?}", e);
        LoginError::BackendError
    })?;

    let ruma_res = LoginResponse::try_from_http_response(res).map_err(|e| {
        error!("Failed to parse response: {}", e);
        LoginError::InvalidCredentials
    })?;

    let refresh_token = ruma_res.refresh_token.clone();
    let expires_in_ms = ruma_res.expires_in.map(|d| d.as_secs() as u64).unwrap_or(0);

    return Ok((
        ClientInfo {
            user_id: ruma_res.user_id.to_string(),
            device_id: ruma_res.device_id.to_string(),
        },
        Token {
            access_token: ruma_res.access_token,
            refresh_token: refresh_token.map(|token| RefreshToken::new(token, expires_in_ms)),
        },
    ));
}

/// Refreshes the access token using the provided refresh token, returning a new access token and refresh token (if supported by the server).
pub async fn refresh_token(
    server_info: HomeServerInfo,
    refresh_token: String,
) -> Result<Token, TauriError> {
    let client = Client::new();
    let payload = RefrefreshRequest::new(refresh_token);

    let req = payload.try_into_http_request::<Vec<u8>>(
        &server_info.base_url.as_str(),
        SendAccessToken::None,
        Cow::Borrowed(&server_info.supported_versions),
    )?;

    let req = reqwest::Request::try_from(req)?;

    let res = client
        .execute(req)
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    let refresh_res =
        RefreshResponse::try_from_http_response(reqwest_response_to_http_response(res).await?)
            .map_err(|e| format!("Failed to parse response: {e}"))?;

    Ok(Token {
        access_token: refresh_res.access_token,
        refresh_token: Some(RefreshToken::new(
            refresh_res.refresh_token.ok_or("No new refresh token")?,
            refresh_res
                .expires_in_ms
                .map(|ms| ms.as_secs())
                .unwrap_or(0),
        )),
    })
}

pub async fn get_account_data(
    token: &String,
    matrix_url: &String,
    user_id: &String,
    data_type: &String,
) -> Result<Value, TauriError> {
    let client = Client::new();

    let url = construct_url(vec![
        matrix_url.as_str(),
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

        return Ok(json_res);
    } else {
        return Err(format!("Web request failed: {}", res.status()).into());
    }
}
