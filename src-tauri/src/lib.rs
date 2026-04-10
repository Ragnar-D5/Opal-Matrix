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
}

#[derive(Serialize, Deserialize)]
struct MatrixLoginResponse {
    user_id: String,
    access_token: String,
}

#[derive(Default)]
struct AppState {
    access_token: Mutex<Option<String>>,
    matrix_url: Mutex<Option<String>>,
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn console_log(message: String) {
    println!("{message}")
}

#[tauri::command]
async fn matrix_login(
    matrix_url: String,
    username: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<MatrixLoginResponse, String> {
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

        let mut guard = state.access_token.lock().await;
        *guard = Some(json_res.access_token.clone());

        return Ok(json_res);
    } else {
        error!("Failed to log in: {}", res.status());

        return Err("Failed to log in".to_string());
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
        .invoke_handler(tauri::generate_handler![greet, console_log, matrix_login])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
