<<<<<<< Updated upstream
=======
use anyhow::Context;
use cpal::traits::{DeviceTrait, StreamTrait};
>>>>>>> Stashed changes
use matrix_sdk::Room;
use matrix_sdk::ruma::{EventId, OwnedEventId};
use matrix_sdk::ruma::{OwnedRoomId, events::room::MediaSource};
use matrix_sdk_ui::timeline::{
    DateDividerMode, TimelineEventFocusThreadMode, TimelineFocus, TimelineReadReceiptTracking,
};
use matrix_sdk_ui::{Timeline, timeline::TimelineBuilder};
<<<<<<< Updated upstream
use std::{collections::HashMap, sync::Arc};
=======
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
>>>>>>> Stashed changes
use tauri::async_runtime::{Mutex, RwLock};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::TauriError;

#[derive(Default)]
pub struct AppState {
    pub frontend_current_room_id: RwLock<Option<String>>,
    pub frontend_is_focused: RwLock<bool>,
}

type TimelineKey = (OwnedRoomId, Option<OwnedEventId>);

pub struct TimelineManager {
    pub timelines: RwLock<HashMap<TimelineKey, (Uuid, Arc<Timeline>)>>,
    pub timelines_by_id: RwLock<HashMap<Uuid, Arc<Timeline>>>,
    pub stream_handle: Mutex<Option<JoinHandle<()>>>,
}

impl Default for TimelineManager {
    fn default() -> Self {
        TimelineManager {
            timelines: RwLock::new(HashMap::new()),
            timelines_by_id: RwLock::new(HashMap::new()),
            stream_handle: Mutex::new(None),
        }
    }
}

impl TimelineManager {
    pub async fn get_or_create_timeline(
        &self,
        room: &Room,
        event_id: Option<OwnedEventId>,
    ) -> Result<(Uuid, Arc<Timeline>), TauriError> {
        let index = (room.room_id().to_owned(), event_id.clone());

        {
            let guard = self.timelines.read().await;
            if let Some((id, timeline)) = guard.get(&index) {
                return Ok((*id, timeline.clone()));
            }
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

        let id = Uuid::new_v4();
        let timeline = Arc::new(timeline);

        self.timelines
            .write()
            .await
            .insert(index, (id, timeline.clone()));
        self.timelines_by_id
            .write()
            .await
            .insert(id, timeline.clone());

        Ok((id, timeline))
    }

    pub async fn get_timeline_by_id(&self, id: Uuid) -> Option<Arc<Timeline>> {
        self.timelines_by_id.read().await.get(&id).cloned()
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

pub type AudioManager = Mutex<AudioManagerContext>;

// Might need to add the configs for the devices, till then it's just default
pub struct AudioManagerContext {
    pub host: cpal::Host,

    pub input_device: Option<cpal::Device>,
    pub input_stream: Option<cpal::Stream>,
    pub input_consumer: Option<HeapCons<i16>>,

    pub output_device: Option<cpal::Device>,
    pub output_stream: Option<cpal::Stream>,
    pub output_producer: Option<HeapProd<f32>>,
}

impl AudioManagerContext {
    pub fn new() -> Self {
        use cpal::traits::HostTrait;
        let host = cpal::default_host();
        let input_device = host.default_input_device();
        let output_device = host.default_output_device();
        Self {
            host,
            input_device,
            input_stream: None,
            input_consumer: None,
            output_device,
            output_stream: None,
            output_producer: None,
        }
    }

    pub async fn try_setup_output_stream(&mut self) -> Result<(), anyhow::Error> {
        let device = self
            .output_device
            .as_ref()
            .context("No output device set")?;
        let config = device.default_output_config().context("No output config")?;

        let sample_rate = config.sample_rate(); // should be 48 000
        let channels = config.channels() as u32;

        log::debug!("CPAL output: {} Hz, {} ch", sample_rate, channels);

        let ring = HeapRb::<f32>::new(sample_rate as usize * channels as usize * 4);
        let (producer, mut consumer) = ring.split();

        // ~50 ms prebuffer
        let prebuffer_threshold = (sample_rate * channels / 50) as usize;
        let mut is_buffering = true;

        let output_stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    if is_buffering {
                        if consumer.occupied_len() >= prebuffer_threshold {
                            is_buffering = false;
                        } else {
                            data.fill(0.0);
                            return;
                        }
                    }
                    for sample in data.iter_mut() {
                        match consumer.try_pop() {
                            Some(s) => *sample = s * 0.85,
                            None => {
                                *sample = 0.0;
                                is_buffering = true;
                            }
                        }
                    }
                },
                |err| log::error!("Speaker stream error: {err}"),
                None,
            )
            .context("Failed to build output stream")?;

        output_stream
            .play()
            .context("Failed to start output stream")?;

        self.output_stream = Some(output_stream);
        self.output_producer = Some(producer);
        Ok(())
    }

    pub async fn try_setup_input_stream(&mut self) -> Result<(), anyhow::Error> {
        let device = self.input_device.as_ref().context("No input device set")?;
        let config = device.default_input_config()?;

        let sample_rate = config.sample_rate(); // Extracts the u32 sample rate
        let channels = config.channels() as u32;

        log::debug!("CPAL input: {} Hz, {} ch", sample_rate, channels);

        let samples_per_10ms = ((sample_rate * channels) / 100) as usize;

        let input_ring = HeapRb::<i16>::new(samples_per_10ms * 8);
        let (mut input_producer, input_consumer) = input_ring.split();

        let input_stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    for &sample in data {
                        let s = (sample * i16::MAX as f32) as i16;
                        // Lock-free, non-blocking push ensures zero dropouts in CPAL
                        let _ = input_producer.try_push(s);
                    }
                },
                |err| log::error!("Mic stream error: {}", err),
                None,
            )
            .context("Failed to build mic stream")?;

        input_stream.play().context("Failed to play mic")?;

        self.input_stream = Some(input_stream);
        self.input_consumer = Some(input_consumer);
        Ok(())
    }
}
