use matrix_sdk::Room;
use matrix_sdk_ui::{Timeline, timeline::TimelineBuilder};
use ruma::{OwnedRoomId, api::SupportedVersions};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::async_runtime::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use matrix_sdk_crypto::OlmMachine;

use crate::{
    TauriError,
    matrix_api::discovery::{Authentication, fetch_supported_versions},
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
                // self.refresh_token().await?;
            }
        }

        let token_guard = self.token.read().await;
        let token = token_guard.as_ref().ok_or("Not logged in")?;

        Ok(token.access_token.clone())
    }

    pub async fn room_id(self: &Arc<Self>) -> Result<Option<String>, TauriError> {
        let room_id_guard = self.frontend_current_room_id.read().await;
        Ok(room_id_guard.clone())
    }
}

pub struct TimelineManager {
    pub timelines: RwLock<HashMap<OwnedRoomId, Arc<Timeline>>>,
    pub stream_handle: Mutex<Option<JoinHandle<()>>>,
}

impl Default for TimelineManager {
    fn default() -> Self {
        TimelineManager {
            timelines: RwLock::new(HashMap::new()),
            stream_handle: Mutex::new(None),
        }
    }
}

impl TimelineManager {
    pub async fn get_or_create_timeline(&self, room: &Room) -> Result<Arc<Timeline>, TauriError> {
        let mut guard = self.timelines.write().await;
        if let Some(timeline) = guard.get(room.room_id()) {
            return Ok(timeline.clone());
        }

        log::debug!("Creating new timeline for room {}", room.room_id());
        let timeline = TimelineBuilder::new(room)
            .with_date_divider_mode(matrix_sdk_ui::timeline::DateDividerMode::Daily)
            .build()
            .await?;
        timeline.paginate_backwards(30).await?;

        let timeline = Arc::new(timeline);

        guard.insert(room.room_id().to_owned(), timeline.clone());
        Ok(timeline)
    }

    pub async fn set_stream_handle(&self, handle: JoinHandle<()>) {
        let mut guard = self.stream_handle.lock().await;
        *guard = Some(handle);
    }

    pub async fn abort_stream(&self) {
        let mut guard = self.stream_handle.lock().await;
        if let Some(handle) = guard.take() {
            handle.abort();
        }
    }
}

pub struct TaskManager {
    pub tasks: Mutex<HashMap<String, CancellationToken>>,
}

impl Default for TaskManager {
    fn default() -> Self {
        TaskManager {
            tasks: Mutex::new(HashMap::new()),
        }
    }
}

impl TaskManager {
    pub async fn replace_task(&self, command_name: &str, new_token: CancellationToken) {
        let mut tasks = self.tasks.lock().await;

        if let Some(old_token) = tasks.insert(command_name.to_string(), new_token) {
            old_token.cancel();
        }
    }
}
