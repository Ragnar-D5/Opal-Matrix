use std::{io::ErrorKind, path::PathBuf};

use matrix_sdk::{
    Client,
    ruma::{
        events::{AnyGlobalAccountDataEvent, GlobalAccountDataEventType},
        serde::Raw,
    },
};
use serde_json::{Value, json};
use tauri::{State, command};
use tokio::sync::RwLock;
use toml_edit::{Array, DocumentMut, InlineTable, Item, value};

use crate::TauriError;

async fn load_setting_from_file(
    settings_file: &PathBuf,
    key: &str,
) -> Result<Option<String>, TauriError> {
    let content = match tokio::fs::read_to_string(settings_file).await {
        Ok(content) => content,
        Err(e) => match e.kind() {
            ErrorKind::NotFound => return Ok(None), // No settings file, treat as empty
            _ => return Err(e.into()),
        },
    };

    let config: Value = toml::from_str(&content)?;

    Ok(config.get(key).map(|s| s.to_string()))
}

fn json_to_toml_item(json_val: Value) -> Option<Item> {
    match json_val {
        Value::Null => None,
        Value::Bool(b) => Some(value(b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(value(i))
            } else {
                n.as_f64().map(value)
            }
        }
        Value::String(s) => Some(value(s)),
        Value::Array(arr) => {
            let mut toml_arr = Array::new();
            for v in arr {
                if let Some(Item::Value(tv)) = json_to_toml_item(v) {
                    toml_arr.push(tv);
                }
            }
            Some(value(toml_arr))
        }
        Value::Object(obj) => {
            let mut table = InlineTable::new();
            for (k, v) in obj {
                if let Some(Item::Value(tv)) = json_to_toml_item(v) {
                    table.insert(&k, tv);
                }
            }
            Some(value(table))
        }
    }
}

async fn save_setting_to_file(
    settings_file: &PathBuf,
    key: &str,
    value: &str,
) -> Result<(), TauriError> {
    if let Some(parent) = settings_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let content = match tokio::fs::read_to_string(settings_file).await {
        Ok(content) => content,
        Err(e) => match e.kind() {
            ErrorKind::NotFound => String::new(), // No settings file, start with empty
            _ => return Err(e.into()),
        },
    };

    let mut doc = content.parse::<DocumentMut>()?;

    let json_value: Value = serde_json::from_str(value)?;

    if let Some(toml_item) = json_to_toml_item(json_value) {
        doc.insert(key, toml_item);
    } else {
        doc.remove(key);
    }

    tokio::fs::write(settings_file, doc.to_string()).await?;

    Ok(())
}

async fn load_setting_from_cloud(client: &Client, key: &str) -> Result<Option<String>, TauriError> {
    let event_opt = client
        .account()
        .account_data_raw(GlobalAccountDataEventType::from(key.to_string()))
        .await?;

    if let Some(event) = event_opt {
        let event_json: Value = serde_json::from_str(event.json().get())?;

        if let Some(val) = event_json.get("content").and_then(|c| c.get("value")) {
            return Ok(Some(val.to_string()));
        }
    }

    Ok(None)
}

async fn save_setting_to_cloud(client: &Client, key: &str, value: &str) -> Result<(), TauriError> {
    let parsed_value: Value = serde_json::from_str(value)?;

    let content = Raw::from_json_string(serde_json::to_string(&json!({ "value": parsed_value }))?)?;

    client
        .account()
        .set_account_data_raw(GlobalAccountDataEventType::from(key.to_string()), content)
        .await
        .map_err(|e| e.into())
        .map(|_| ())
}

#[command(rename_all = "snake_case")]
pub async fn get_setting(
    settings_file: State<'_, PathBuf>,
    client: State<'_, RwLock<Client>>,
    key: String,
    from_cloud: bool,
) -> Result<Option<String>, TauriError> {
    let settings_file = settings_file.inner();
    let client: Client = client.read().await.clone();

    let setting = if from_cloud {
        match load_setting_from_cloud(&client, &key).await {
            Ok(Some(value)) => {
                if let Err(e) = save_setting_to_file(settings_file, &key, &value).await {
                    log::warn!(
                        "Failed to save cloud setting to file: {:?}. Continuing with cloud value.",
                        e
                    );
                };
                Some(value)
            }
            Ok(None) => load_setting_from_file(settings_file, &key).await?,
            Err(e) => {
                log::warn!(
                    "Failed to load setting from cloud: {:?}. Falling back to file.",
                    e
                );
                load_setting_from_file(settings_file, &key).await?
            }
        }
    } else {
        load_setting_from_file(settings_file, &key).await?
    };

    Ok(setting)
}

#[command(rename_all = "snake_case")]
pub async fn set_setting(
    settings_file: State<'_, PathBuf>,
    client: State<'_, RwLock<Client>>,
    key: String,
    value: String,
    to_cloud: bool,
) -> Result<(), TauriError> {
    let settings_file = settings_file.inner();
    let client: Client = client.read().await.clone();

    save_setting_to_file(settings_file, &key, &value).await?;

    if to_cloud {
        save_setting_to_cloud(&client, &key, &value).await?;
    }

    Ok(())
}
