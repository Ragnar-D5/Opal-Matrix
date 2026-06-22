use anyhow::Context;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, DeviceId, Stream, SupportedStreamConfig};
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::prelude::{AudioFrame, AudioSourceOptions};
use matrix_sdk::ruma::{events::room::MediaSource, OwnedRoomId};
use matrix_sdk::ruma::{EventId, OwnedEventId};
use matrix_sdk::Room;
use matrix_sdk_ui::timeline::{
    DateDividerMode, TimelineEventFocusThreadMode, TimelineFocus, TimelineReadReceiptTracking,
};
use matrix_sdk_ui::{timeline::TimelineBuilder, Timeline};
use ringbuf::traits::{Consumer, Observer, Split};
use ringbuf::{HeapProd, HeapRb};
use std::{collections::HashMap, sync::Arc};
use tauri::async_runtime::{Mutex, RwLock};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
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

pub struct AudioManager {
    pub host: cpal::Host,

    pub input_devices: Mutex<HashMap<DeviceId, cpal::Device>>,
    pub output_devices: Mutex<HashMap<DeviceId, cpal::Device>>,

    pub input_device: Mutex<Option<Device>>,
    pub output_device: Mutex<Option<Device>>,

    input_stream: Mutex<Option<Stream>>,
    output_stream: Mutex<Option<Stream>>,

    input_sender: UnboundedSender<Option<(UnboundedReceiver<f32>, SupportedStreamConfig)>>,
    pub output_producer: Mutex<Option<HeapProd<f32>>>,

    pub native_audio_source: Arc<Mutex<Option<NativeAudioSource>>>,
}

fn get_devices(host: &cpal::Host) -> (HashMap<DeviceId, Device>, HashMap<DeviceId, Device>) {
    let input_devices = match host.input_devices() {
        Ok(devices) => devices
            .into_iter()
            .filter_map(|d| {
                let id = d.id().ok()?;
                Some((id, d))
            })
            .collect(),
        Err(e) => {
            log::warn!("No input devices found: {e}");
            HashMap::new()
        }
    };

    let output_devices = match host.output_devices() {
        Ok(devices) => devices
            .into_iter()
            .filter_map(|d| {
                let id = d.id().ok()?;
                Some((id, d))
            })
            .collect(),
        Err(e) => {
            log::warn!("No output devices found: {e}");
            HashMap::new()
        }
    };

    (input_devices, output_devices)
}

use cpal::traits::HostTrait;
impl AudioManager {
    pub fn refresh_devices(&self) -> Result<(), TauriError> {
        let (input_devices, output_devices) = get_devices(&self.host);

        let input_id = self
            .input_device
            .blocking_lock()
            .clone()
            .map(|d| d.id().ok())
            .flatten();
        if let Some(id) = input_id {
            if !input_devices.contains_key(&id) {
                if let Some(id) = self.host.default_input_device() {
                    self.try_setup_input_stream_for_device(&id)?;
                } else {
                    *self.input_device.blocking_lock() = None;
                    *self.input_stream.blocking_lock() = None;
                };
            }
        }

        let output_id = self
            .output_device
            .blocking_lock()
            .clone()
            .map(|d| d.id().ok())
            .flatten();
        if let Some(id) = output_id {
            if !output_devices.contains_key(&id) {
                if let Some(id) = self.host.default_output_device() {
                    self.try_setup_input_stream_for_device(&id)?;
                } else {
                    *self.output_device.blocking_lock() = None;
                    *self.output_stream.blocking_lock() = None;
                    *self.output_producer.blocking_lock() = None;
                }
            }
        }
        Ok(())
    }

    pub fn new() -> Self {
        let host = cpal::default_host();

        let (input_sender, input_receiver) = mpsc::unbounded_channel();
        let new = Self {
            host,
            input_sender,

            input_devices: Mutex::new(HashMap::new()),
            output_devices: Mutex::new(HashMap::new()),

            input_stream: Mutex::new(None),

            output_stream: Mutex::new(None),
            output_producer: Mutex::new(None),

            input_device: Mutex::new(None),
            output_device: Mutex::new(None),

            native_audio_source: Arc::new(Mutex::new(None)),
        };
        new.setup_global_input_handler(input_receiver);
        new
    }

    pub fn try_setup_output_stream_for_device(&self, device: &Device) -> Result<(), TauriError> {
        *self.input_device.blocking_lock() = Some(device.clone());

        let config = device.default_output_config().context("No output config")?;

        let sample_rate = config.sample_rate(); // should be 48 000
        let channels = config.channels() as u32;

        log::debug!("CPAL output: {} Hz, {} ch", sample_rate, channels);

        let ring = HeapRb::<f32>::new(sample_rate as usize * channels as usize * 4);
        let (producer, mut consumer) = ring.split();

        // ~50 ms prebuffer
        let prebuffer_threshold = (sample_rate * channels / 50) as usize;
        let mut is_buffering = true;

        let output_stream = device.build_output_stream(
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
        )?;

        output_stream.play()?;

        *self.output_stream.blocking_lock() = Some(output_stream);
        *self.output_producer.blocking_lock() = Some(producer);
        Ok(())
    }

    pub fn try_setup_input_stream_for_device(&self, device: &Device) -> Result<(), TauriError> {
        let (rx, tx) = mpsc::unbounded_channel();

        let config = device.default_input_config()?;

        let input_stream = device.build_input_stream(
            &config.clone().into(),
            move |data: &[f32], _| {
                for sample in data {
                    rx.send(*sample);
                }
            },
            |err| log::error!("Mic stream error: {}", err),
            None,
        )?;

        input_stream.play()?;

        self.input_sender.send(Some((tx, config.clone())))?;
        *self.input_stream.blocking_lock() = Some(input_stream);
        Ok(())
    }

    pub fn setup_global_input_handler(
        &self,
        mut input_data_receiver: UnboundedReceiver<
            Option<(UnboundedReceiver<f32>, SupportedStreamConfig)>,
        >,
    ) {
        let native_audio_source = self.native_audio_source.clone();

        std::thread::spawn(async move || loop {
            let Some(next) = input_data_receiver.blocking_recv() else {
                return;
            };
            let Some((mut data_receiver, config)) = next else {
                return;
            };

            let sample_rate = config.sample_rate();
            let channels = config.channels() as u32;
            let samples_per_10ms = (sample_rate / 100) as usize;

            *native_audio_source.blocking_lock() = Some(NativeAudioSource::new(
                AudioSourceOptions::default(),
                sample_rate,
                channels,
                10,
            ));

            let mut frame_buffer = Vec::with_capacity(samples_per_10ms);
            loop {
                while frame_buffer.len() < samples_per_10ms {
                    if let Some(s) = data_receiver.blocking_recv() {
                        frame_buffer.push(s as i16);
                    } else {
                        break;
                    }
                }

                let frame = AudioFrame {
                    data: frame_buffer.clone().into(),
                    sample_rate,
                    num_channels: channels,
                    samples_per_channel: samples_per_10ms as u32,
                };
                frame_buffer.clear();

                if let Some(source) = native_audio_source.blocking_lock().as_ref() {
                    if let Err(e) = source.capture_frame(&frame).await {
                        log::error!("Failed to push audio frame: {e}");
                    }
                }
            }
        });
    }
}
