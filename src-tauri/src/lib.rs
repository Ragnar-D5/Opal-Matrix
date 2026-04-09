use std::sync::Mutex;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::State;

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

#[derive(Deserialize)]
struct MatrixLoginResponse {
    access_token: Option<String>,
    user_id: Option<String>,
}

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
async fn start_function() -> Result<(), String> {
    let client = Client::new();

    let matrix_url = "https://matrix.erik-is.gay".to_string();

    let payload = MatrixLoginRequest {
        login_type: "m.login.password".to_string(),
        identifier: MatrixLoginIdentifier {
            id_type: "m.id.user".to_string(),
            user: "username".to_string(),
        },
        password: "password".to_string(),
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

        println!("{:?}", json_res.user_id);
        println!("{:?}", json_res.access_token);
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet, console_log, start_function])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
