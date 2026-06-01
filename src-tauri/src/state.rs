use matrix_sdk::ruma::OwnedRoomId;
use matrix_sdk::Room;
use matrix_sdk_ui::{timeline::TimelineBuilder, Timeline};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tauri::async_runtime::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::TauriError;

#[derive(Default)]
pub struct AppState {
    pub app_data_dir: PathBuf,

    pub frontend_current_room_id: RwLock<Option<String>>,
    pub frontend_is_focused: RwLock<bool>,
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

#[derive(Default)]
pub struct CallAudioState {
    pub input_stream: Mutex<Option<cpal::Stream>>,
    pub output_stream: Mutex<Option<cpal::Stream>>,
}
