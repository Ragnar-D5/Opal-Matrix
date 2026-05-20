use log::info;
use matrix_sdk::Room;
use matrix_sdk_ui::{timeline::TimelineBuilder, Timeline};
use ruma::{api::SupportedVersions, OwnedRoomId, UserId};
use rusqlite::Connection;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    async_runtime::{JoinHandle, Mutex, RwLock},
    AppHandle,
};
use tauri_plugin_http::reqwest::Client;
use url::Url;

use matrix_sdk_crypto::{CrossSigningKeyExport, OlmMachine};
use tokio_util::sync::CancellationToken;

use crate::{
    construct_url,
    matrix_api::{
        authentication::{self, get_account_data},
        crypto::{self, StoredSession},
        discovery::{fetch_supported_versions, Authentication},
    },
    storage, TauriError,
};

#[derive(Default, Clone)]
pub struct RefreshToken {
    token: String,
    expires_at: u64,
}

impl RefreshToken {
    pub fn new(token: String, ms: u64) -> Self {
        RefreshToken {
            token,
            expires_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Failed to get time")
                .as_secs()
                + ms,
        }
    }
}

#[derive(Default, Clone)]
pub struct Token {
    pub access_token: String,

    pub refresh_token: Option<RefreshToken>,
}

impl Token {
    fn is_stale(&self) -> bool {
        let expires_at = if let Some(refresh) = &self.refresh_token {
            refresh.expires_at
        } else {
            return false;
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Failed to get time")
            .as_secs();

        now + 30 >= expires_at
    }
}

#[derive(Default, Clone)]
pub struct ClientInfo {
    pub user_id: String,
    pub device_id: String,
}

#[derive(Clone)]
pub struct HomeServerInfo {
    pub base_url: String,
    pub supported_versions: SupportedVersions,
}

impl HomeServerInfo {
    pub async fn try_new(base_url: String) -> Result<Self, TauriError> {
        Ok(HomeServerInfo {
            base_url: base_url.clone(),
            supported_versions: fetch_supported_versions(&base_url).await?,
        })
    }
}

#[derive(Default)]
pub struct AppState {
    pub app_data_dir: PathBuf,

    pub token: RwLock<Option<Token>>,
    pub client: RwLock<Option<ClientInfo>>,

    pub next_batch: RwLock<Option<String>>,

    pub home_server_info: RwLock<Option<HomeServerInfo>>,
    pub auth_provider: RwLock<Option<Authentication>>,

    pub refresh_lock: Mutex<()>,

    pub crypto_machine: Mutex<Option<OlmMachine>>,
    pub recovery_key: RwLock<Option<String>>,

    pub sync_task: Mutex<Option<JoinHandle<()>>>,
    pub sync_cancel_token: Mutex<Option<CancellationToken>>,

    pub connection: Mutex<Option<rusqlite::Connection>>,

    pub frontend_current_room_id: RwLock<Option<String>>,
    pub frontend_is_focused: RwLock<bool>,

    pub messages_to_delete: RwLock<HashMap<String, String>>,
}

impl AppState {
    pub async fn save_session(&self) -> Result<(), TauriError> {
        let token = {
            let token_guard = self.token.read().await;
            token_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let client_info = {
            let client_guard = self.client.read().await;
            client_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let matrix_url = {
            let url_guard = self.home_server_info.read().await;
            url_guard.as_ref().ok_or("Not logged in")?.clone().base_url
        };

        let session = crypto::StoredSession {
            user_id: client_info.user_id.clone(),
            device_id: client_info.device_id.clone(),
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token.clone().map(|r| r.token),
            expires_at: token.refresh_token.clone().map(|r| r.expires_at),
            next_batch: self.next_batch.read().await.clone(),
            recovery_key: self.recovery_key.read().await.clone(),
            homeserver_url: matrix_url.to_string(),
        };

        crypto::save_session(&session).await?;

        Ok(())
    }

    async fn refresh_token(&self) -> Result<(), TauriError> {
        let refresh_token = {
            let token_guard = self.token.read().await;
            let token = token_guard.as_ref().ok_or("Not logged in")?.clone();

            if let Some(refresh) = token.refresh_token {
                refresh
            } else {
                return Err("No refresh token available".into());
            }
        };

        let server_info = self
            .home_server_info
            .read()
            .await
            .clone()
            .ok_or("Not logged in")?;

        let res = authentication::refresh_token(server_info, refresh_token.token).await?;

        {
            let mut write_guard = self.token.write().await;
            *write_guard = Some(res);
        }

        self.save_session().await?;

        return Ok(());
    }

    pub async fn get_last_session(&self) -> Result<Option<StoredSession>, TauriError> {
        crypto::get_last_active_session().await
    }

    pub async fn login_or_restore_session(
        &self,
        session: StoredSession,
    ) -> Result<Option<String>, TauriError> {
        let token = Token {
            access_token: session.access_token,
            refresh_token: session.refresh_token.map(|r| RefreshToken {
                token: r,
                expires_at: session.expires_at.unwrap_or(0),
            }),
        };

        let client_info = ClientInfo {
            user_id: session.user_id.clone(),
            device_id: session.device_id.clone(),
        };

        {
            let mut token_guard = self.token.write().await;
            *token_guard = Some(token);

            let mut client_guard = self.client.write().await;
            *client_guard = Some(client_info);

            let mut server_guard = self.home_server_info.write().await;
            *server_guard = Some(HomeServerInfo::try_new(session.homeserver_url).await?);

            let mut recovery_guard = self.recovery_key.write().await;
            *recovery_guard = session.recovery_key;

            let mut next_batch_guard = self.next_batch.write().await;
            *next_batch_guard = session.next_batch;
        }

        if self.check_token().await.is_err() {
            return Ok(None);
        } else {
            return Ok(Some(session.user_id));
        }
    }

    /// Checks if the current token is valid and refreshes it if necessary, returning the access token.
    pub async fn check_token(&self) -> Result<String, TauriError> {
        let needs_refresh = {
            let token_guard = self.token.read().await;
            let t = token_guard.as_ref().ok_or("Not logged in")?;
            t.is_stale()
        };

        if needs_refresh {
            let _lock = self.refresh_lock.lock().await;

            // Check if another thread already refreshed it
            let still_needs_refresh = {
                let token_guard = self.token.read().await;
                token_guard.as_ref().map(|t| t.is_stale()).unwrap_or(false)
            };

            if still_needs_refresh {
                self.refresh_token().await?;
            }
        }

        let token_guard = self.token.read().await;
        let token = token_guard.as_ref().ok_or("Not logged in")?;

        Ok(token.access_token.clone())
    }

    pub async fn init_stuff(self: &Arc<Self>, handle: &AppHandle) -> Result<(), TauriError> {
        let client_info = {
            let client_guard = self.client.read().await;
            client_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        let db_passphrase = crypto::get_or_create_passphrase(client_info.user_id.clone()).await?;

        // let machine = crypto::init_crypto_machine(
        //     self.app_data_dir.clone(),
        //     client_info.user_id.clone(),
        //     client_info.device_id.clone(),
        //     db_passphrase.clone(),
        // )
        // .await?;

        let mut machine_guard = self.crypto_machine.lock().await;
        // *machine_guard = Some(machine);

        let (already_loaded, conn) = storage::init_storage(
            self.app_data_dir.clone(),
            &client_info.device_id.clone(),
            &db_passphrase,
        )
        .await?;

        {
            let mut conn_guard = self.connection.lock().await;
            *conn_guard = Some(conn);
        }
        self.start_sync(handle, !already_loaded).await?;

        Ok(())
    }

    pub async fn set_recovery_key(&self, recovery_key: String) -> Result<(), TauriError> {
        {
            let mut key_guard = self.recovery_key.write().await;
            *key_guard = Some(recovery_key.clone());
        }

        let matrix_url = {
            let server_guard = self.home_server_info.read().await;
            server_guard
                .as_ref()
                .ok_or("Not logged in")?
                .clone()
                .base_url
        };

        let olm_machine = {
            let lock_guard = self.crypto_machine.lock().await;
            lock_guard
                .as_ref()
                .ok_or("Crypto machine not initialized")?
                .clone()
        };

        let token = self.check_token().await?;

        // crypto::set_room_keys(&olm_machine, &matrix_url, &token, &recovery_key).await?;

        let client_info = self.client.read().await.clone().ok_or("Not logged in")?;

        // let device = olm_machine
        //     .get_device(
        //         &UserId::parse(client_info.user_id.as_str()).expect("Failted to parse user id"),
        //         client_info.device_id.as_str().into(),
        //         Some(Duration::from_secs(10)),
        //     )
        //     .await?
        //     .ok_or("No device for some reason")?;

        // info!("Device {:?}", device);

        let res = get_account_data(
            &token,
            &matrix_url,
            &olm_machine.user_id().to_string(),
            &"m.secret_storage.default_key".to_string(),
        )
        .await?;

        let default_key_id = res["key"]
            .as_str()
            .ok_or("Missing default_key in account data")?;
        let res = get_account_data(
            &token,
            &matrix_url,
            &olm_machine.user_id().to_string(),
            &"m.cross_signing.self_signing".to_string(),
        )
        .await?;

        let enc = &res["encrypted"][default_key_id];

        let ciphertext = enc["ciphertext"]
            .as_str()
            .ok_or("Missing ciphertext in encrypted key data")?;
        let mac = enc["mac"]
            .as_str()
            .ok_or("Missing mac in encrypted key data")?;
        let iv = enc["iv"]
            .as_str()
            .ok_or("Missing ephemeral in encrypted key data")?;

        // let self_signing_key = decrypt_ssss_aes_hmac_sha2(
        //     recovery_key.as_str(),
        //     "m.cross_signing.self_signing",
        //     ciphertext,
        //     iv,
        //     mac,
        // )?;

        // let import = CrossSigningKeyExport {
        //     self_signing_key: Some(self_signing_key),
        //     master_key: None,
        //     user_signing_key: None,
        // };

        // olm_machine.import_cross_signing_keys(import).await?;

        // let upload_request = device.verify().await?;
        // let url = format!("{matrix_url}/_matrix/client/v3/keys/signatures/upload");

        // Client::new()
        //     .post(url)
        //     .bearer_auth(token)
        //     .json(&serde_json::to_value(upload_request.signed_keys).unwrap())
        //     .send()
        //     .await?;

        Ok(())
    }

    /// Returns the active authentication token and server info
    pub async fn get_api(self: &Arc<Self>) -> Result<(String, HomeServerInfo), TauriError> {
        let token = self.check_token().await?;

        let server_info = {
            let server_guard = self.home_server_info.read().await;
            server_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        Ok((token, server_info))
    }

    pub async fn get_api_with_url<T>(
        self: &Arc<Self>,
        parts: Vec<T>,
    ) -> Result<(String, Url), TauriError>
    where
        T: AsRef<str>,
    {
        let token = self.check_token().await?;
        let matrix_url = {
            let url_guard = self.home_server_info.read().await;
            url_guard.as_ref().ok_or("Not logged in")?.clone().base_url
        };

        let all_parts: Vec<String> = std::iter::once(matrix_url)
            .chain(parts.into_iter().map(|s| s.as_ref().to_string()))
            .collect();

        let url = construct_url(all_parts)?;

        Ok((token, url))
    }

    pub async fn start_sync(
        self: &std::sync::Arc<Self>,
        app_handle: &AppHandle,
        resync: bool,
    ) -> Result<(), TauriError> {
        let mut task_guard = self.sync_task.lock().await;
        if task_guard.is_some() {
            return Ok(());
        }

        let cancel = CancellationToken::new();
        {
            let mut cancel_guard = self.sync_cancel_token.lock().await;
            *cancel_guard = Some(cancel.clone());
        }

        let state = self.clone();
        let app_handle = app_handle.clone();
        // let handle = tauri::async_runtime::spawn(async move {
        //     if let Err(e) = run_sync_loop(state, app_handle, resync).await {
        //         log::error!("Sync loop error: {:?}", e);
        //     }
        // });

        // *task_guard = Some(handle);
        Ok(())
    }

    pub async fn stop_sync(&self) -> Result<(), TauriError> {
        if let Some(cancel) = self.sync_cancel_token.lock().await.take() {
            cancel.cancel();
        }

        if let Some(handle) = self.sync_task.lock().await.take() {
            let _ = handle.await;
        }

        Ok(())
    }

    pub async fn restart_sync(self: &Arc<Self>, handle: &AppHandle) -> Result<(), TauriError> {
        self.stop_sync().await?;
        self.start_sync(handle, true).await
    }

    pub async fn user_id(self: &Arc<Self>) -> Result<String, TauriError> {
        let client_info = {
            let client_guard = self.client.read().await;
            client_guard.as_ref().ok_or("Not logged in")?.clone()
        };

        Ok(client_info.user_id)
    }

    pub async fn with_connection_mut<F, R>(self: &Arc<Self>, f: F) -> Result<R, TauriError>
    where
        F: FnOnce(&mut Connection) -> Result<R, TauriError>,
    {
        let mut conn_guard = self.connection.lock().await;
        let conn = conn_guard.as_mut().ok_or("Storage not initialized")?;
        f(conn)
    }

    pub async fn with_connection<F, R>(self: &Arc<Self>, f: F) -> Result<R, TauriError>
    where
        F: FnOnce(&Connection) -> Result<R, TauriError>,
    {
        let conn_guard = self.connection.lock().await;
        let conn = conn_guard.as_ref().ok_or("Storage not initialized")?;
        f(conn)
    }

    pub async fn with_connection_async<F, Fut, R>(self: &Arc<Self>, f: F) -> Result<R, TauriError>
    where
        F: FnOnce(&Connection) -> Fut,
        Fut: std::future::Future<Output = Result<R, TauriError>>,
    {
        let conn_guard = self.connection.lock().await;
        let conn = conn_guard.as_ref().ok_or("Storage not initialized")?;
        f(conn).await
    }

    pub async fn room_id(self: &Arc<Self>) -> Result<Option<String>, TauriError> {
        let room_id_guard = self.frontend_current_room_id.read().await;
        Ok(room_id_guard.clone())
    }
}

pub struct TimelineManager {
    pub timelines: RwLock<HashMap<OwnedRoomId, Arc<Timeline>>>,
}

impl TimelineManager {
    pub fn new() -> Self {
        TimelineManager {
            timelines: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get_or_create_timeline(&self, room: &Room) -> Result<Arc<Timeline>, TauriError> {
        let mut guard = self.timelines.write().await;
        if let Some(timeline) = guard.get(room.room_id()) {
            return Ok(timeline.clone());
        }

        let timeline = TimelineBuilder::new(room)
            .with_date_divider_mode(matrix_sdk_ui::timeline::DateDividerMode::Daily)
            .build()
            .await?;

        let timeline = Arc::new(timeline);

        guard.insert(room.room_id().to_owned(), timeline.clone());
        Ok(timeline)
    }
}
