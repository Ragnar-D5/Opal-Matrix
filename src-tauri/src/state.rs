use matrix_sdk::ruma::{events::room::MediaSource, OwnedRoomId};
use matrix_sdk::ruma::{EventId, OwnedEventId};
use matrix_sdk::Room;
use matrix_sdk_ui::timeline::{
    DateDividerMode, TimelineEventFocusThreadMode, TimelineFocus, TimelineReadReceiptTracking,
};
use matrix_sdk_ui::{timeline::TimelineBuilder, Timeline};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use tauri::async_runtime::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::TauriError;

#[derive(Default)]
pub struct AppState {
    pub app_data_dir: PathBuf,

    pub frontend_current_room_id: RwLock<Option<String>>,
    pub frontend_is_focused: RwLock<bool>,
}

type TimelineKey = (OwnedRoomId, Option<OwnedEventId>);

pub struct TimelineManager {
    pub timelines: RwLock<HashMap<TimelineKey, Arc<Timeline>>>,
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
    pub async fn get_or_create_timeline(
        &self,
        room: &Room,
        event_id: Option<OwnedEventId>,
    ) -> Result<Arc<Timeline>, TauriError> {
        let mut guard = self.timelines.write().await;

        let index = (room.room_id().to_owned(), event_id.clone());

        if let Some(timeline) = guard.get(&index) {
            return Ok(timeline.clone());
        }

        let focus = if let Some(event_id) = &event_id {
            log::debug!(
                "Creating timeline focused on event {} in room {}",
                event_id,
                room.room_id()
            );
            TimelineFocus::Event {
                target: EventId::parse(event_id)?,
                num_context_events: 30,
                thread_mode: TimelineEventFocusThreadMode::Automatic {
                    hide_threaded_events: false,
                },
            }
        } else {
            log::debug!("Creating timeline in room {}", room.room_id());
            TimelineFocus::Live {
                hide_threaded_events: false,
            }
        };

        let timeline = TimelineBuilder::new(room)
            .with_date_divider_mode(DateDividerMode::Daily)
            .with_focus(focus)
            .track_read_marker_and_receipts(TimelineReadReceiptTracking::AllEvents)
            .add_failed_to_parse(true)
            .build()
            .await?;

        if let Some(event_id) = &event_id {
            timeline.fetch_details_for_event(event_id).await?;
        } else {
            timeline.paginate_backwards(30).await?;
        }

        let timeline = Arc::new(timeline);

        guard.insert(index, timeline.clone());
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

#[derive(Clone)]
pub struct MediaManager {
    pub sources: Arc<RwLock<HashMap<Uuid, MediaSource>>>,
}

impl Default for MediaManager {
    fn default() -> Self {
        MediaManager {
            sources: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

use livekit::Room as LiveKitRoom;

pub type LiveKitRoomManager = Arc<Mutex<HashMap<String, LiveKitRoomData>>>; // we can make this more efficient later, but since you probably only interact with one call at a time, this should suffice for now

pub struct LiveKitRoomData {
    pub livekit_room: LiveKitRoom,
    pub cancellation_token: CancellationToken,
    pub key_index: i32,
    pub call_id: Uuid, // why the hell do we need this? This has no usage
}

impl LiveKitRoomData {
    /// the returned future will finish, when the event stream is closed
    pub fn close_event_stream(&self) -> tokio_util::sync::WaitForCancellationFuture<'_> {
        self.cancellation_token.cancel();
        self.cancellation_token.cancelled()
    }
}
