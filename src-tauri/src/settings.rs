use std::{path::PathBuf};

use matrix_sdk::{
    event_handler::Ctx,
    ruma::{
        events::{AnyGlobalAccountDataEvent, GlobalAccountDataEventType},
        serde::Raw,
    },
    Client,
};
use serde_json::{json, Value};
use shared::api::events::SettingsUpdate;
use tauri::{AppHandle, State, command};
use tokio::sync::RwLock;
use toml_edit::{value, Array, DocumentMut, InlineTable, Item};

use crate::{TauriError, send_event};

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
    cashed_settings: &mut DocumentMut,
    key: &str,
    value: &str,
) -> Result<(), TauriError> {
    if let Some(parent) = settings_file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let json_value: Value = serde_json::from_str(value)?;

    if let Some(toml_item) = json_to_toml_item(json_value) {
        cashed_settings.insert(key, toml_item);
    } else {
        cashed_settings.remove(key);
    }

    tokio::fs::write(settings_file, cashed_settings.to_string()).await?;

    Ok(())
}

async fn load_setting_from_cloud(client: &Client, key: &str) -> Result<Option<String>, TauriError> {
    let event_opt = client
        .account()
        .account_data_raw(GlobalAccountDataEventType::from(key.to_string()))
        .await?;

    if let Some(event) = event_opt {
        let content_json: Value = serde_json::from_str(event.json().get())?;

        if let Some(val) = content_json.get("value")
        && !val.is_null() {
            return Ok(Some(val.to_string()));
        }
    }

    Ok(None)
}

pub async fn save_setting_to_cloud(client: &Client, key: &str, value: &str) -> Result<(), TauriError> {
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
    cashed_settings_sig: State<'_, RwLock<DocumentMut>>,
    client: State<'_, RwLock<Client>>,
    key: String,
    from_cloud: bool,
) -> Result<Option<String>, TauriError> {
    let settings_file = settings_file.inner();
    let client: Client = client.read().await.clone();

    let local_val = {
        let cashed_settings = cashed_settings_sig.inner().read().await;
        cashed_settings.get(&key).map(|v| v.to_string())
    };

    if from_cloud {
        let setting = match load_setting_from_cloud(&client, &key).await {
            Ok(Some(value)) => Some(value),
            Ok(None) => {
                if let Some(ref s) = local_val
                    && let Err(e) = save_setting_to_cloud(&client, &key, s).await
                {
                    log::warn!(
                        "Failed to save local setting to cloud: {:?}. Continuing with local value.",
                        e
                    );
                }
                local_val
            }
            Err(e) => {
                log::warn!(
                    "Failed to load setting from cloud: {:?}. Falling back to file.",
                    e
                );
                local_val
            }
        };
        let mut cashed_settings_mut = cashed_settings_sig.inner().write().await;
        if let Some(ref s) = setting
            && let Err(e) = save_setting_to_file(settings_file, &mut cashed_settings_mut, &key, s).await
        {
            log::warn!(
                "Failed to save cloud setting to file: {:?}. Continuing with cloud value.",
                e
            );
        };
        Ok(setting)
    } else {
        Ok(local_val)
    }
}

#[command(rename_all = "snake_case")]
pub async fn set_setting(
    app_handle: AppHandle,
    settings_file: State<'_, PathBuf>,
    cashed_settings_sig: State<'_, RwLock<DocumentMut>>,
    client: State<'_, RwLock<Client>>,
    key: String,
    value: String,
    to_cloud: bool,
) -> Result<(), TauriError> {
    let settings_file = settings_file.inner();
    let client: Client = client.read().await.clone();

    {
        let mut cashed_settings_mut = cashed_settings_sig.inner().write().await;
        save_setting_to_file(settings_file, &mut cashed_settings_mut, &key, &value).await?;
    } // release write lock before the network call

    // Notify the frontend directly. The file watcher will see no diff (cache
    // and file are always in sync), so we must emit the event ourselves.
    send_event(&app_handle, &SettingsUpdate {
        key: key.clone(),
        value: value.clone(),
        cloud: false,
        // skip_cloud_upload=true because we handle cloud below; the frontend
        // must not re-upload or it causes a redundant round-trip.
        skip_cloud_upload: true,
    });

    if to_cloud {
        save_setting_to_cloud(&client, &key, &value).await?;
    }

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn set_setting_cloud(client: State<'_, RwLock<Client>>, key: String, value: String) -> Result<(), TauriError> {
    let client: Client = client.read().await.clone();
    save_setting_to_cloud(&client, &key, &value).await
}

pub fn document_to_json_map(doc: &DocumentMut) -> serde_json::Map<String, Value> {
    toml::from_str::<toml::Value>(&doc.to_string())
        .ok()
        .and_then(|v| serde_json::to_value(v).ok())
        .and_then(|v| match v {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

pub async fn handle_account_data_event(
    event: Raw<AnyGlobalAccountDataEvent>,
    handle: Ctx<AppHandle>,
) {
    let Ok(event_json): Result<Value, _> = serde_json::from_str(event.json().get()) else {
        return;
    };

    let Some(key) = event_json.get("type").and_then(|t| t.as_str()) else {
        return;
    };

    if !key.starts_with("com.opal") {
        return;
    }

    let Some(val) = event_json.get("content").and_then(|c| c.get("value")) else {
        return;
    };

    if val.is_null() {
        return;
    }

    send_event(&handle, &SettingsUpdate {
        key: key.to_string(),
        value: val.to_string(),
        cloud: true,
        skip_cloud_upload: false,
    });
}
