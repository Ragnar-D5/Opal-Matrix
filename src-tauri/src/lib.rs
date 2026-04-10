use log::{error, info, trace};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::async_runtime::Mutex;
use tauri::{Manager, State};

#[derive(Serialize)]
struct MatrixLoginIdentifier {
    #[serde(rename = "type")]
    id_type: String,
    user: String,
}

#[derive(Serialize)]
struct MatrixLoginRequest {
    #[serde(rename = "type")]
    login_type: String,
    identifier: MatrixLoginIdentifier,
    password: String,
    refresh_token: bool,
}

#[derive(Serialize, Deserialize)]
struct MatrixLoginResponse {
    user_id: String,
    device_id: String,

    access_token: String,
    refresh_token: String,
    expires_in_ms: u64,
}

#[derive(Default)]
struct TokenInfo {
    access_token: String,
    refresh_token: String,
    expires_in_ms: u64,
}

#[derive(Default)]
struct ClientInfo {
    user_id: String,
    device_id: String,
}

#[derive(Default)]
struct AppState {
    token: Mutex<Option<TokenInfo>>,
    client: Mutex<Option<ClientInfo>>,

    matrix_url: Mutex<Option<String>>,
}

#[derive(serde::Serialize)]
enum TauriError {
    Wrap(String),
}

impl From<anyhow::Error> for TauriError {
    fn from(value: anyhow::Error) -> Self {
        Self::Wrap(value.to_string())
    }
}

impl From<String> for TauriError {
    fn from(value: String) -> Self {
        Self::Wrap(value)
    }
}

impl<T> From<Result<T, String>> for TauriError {
    fn from(value: Result<T, String>) -> Self {
        Self::Wrap(value.err().unwrap_or("Unknown error".to_string()))
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn matrix_login(
    matrix_url: String,
    username: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<MatrixLoginResponse, TauriError> {
    let client = Client::new();

    trace!("Getting login");

    let mut guard = state.matrix_url.lock().await;
    *guard = Some(matrix_url.clone());

    let payload = MatrixLoginRequest {
        login_type: "m.login.password".to_string(),
        identifier: MatrixLoginIdentifier {
            id_type: "m.id.user".to_string(),
            user: username,
        },
        password: password,
        refresh_token: true,
    };

    let res = client
        .post(format!("{matrix_url}/_matrix/client/v3/login"))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if res.status().is_success() {
        let json_res: MatrixLoginResponse = res
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))
            .unwrap();

        info!("Successfully logged in as {}", json_res.user_id);

        let mut guard = state.token.lock().await;
        *guard = Some(TokenInfo {
            access_token: json_res.access_token.clone(),
            refresh_token: json_res.refresh_token.clone(),
            expires_in_ms: json_res.expires_in_ms,
        });

        let mut guard = state.client.lock().await;
        *guard = Some(ClientInfo {
            user_id: json_res.user_id.clone(),
            device_id: json_res.device_id.clone(),
        });

        return Ok(json_res);
    } else {
        error!("Failed to log in: {}", res.status());

        return Err("Failed to log in".to_string().into());
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::default();

    tauri::Builder::default()
        .setup(|app| {
            app.manage(Mutex::new(AppState::default()));
            Ok(())
        })
        .manage(state)
        .plugin(tauri_plugin_log::Builder::new().build())
        .invoke_handler(tauri::generate_handler![matrix_login])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
