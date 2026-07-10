use cpal::{Device, DeviceId, SampleFormat, Stream, SupportedStreamConfig, traits::{DeviceTrait, HostTrait, StreamTrait}};
use livekit::webrtc::{audio_source::native::NativeAudioSource, prelude::{AudioFrame, AudioSourceOptions}};
use matrix_sdk::{Room, ruma::{EventId, OwnedEventId, OwnedRoomId, events::room::MediaSource}};
use matrix_sdk_ui::{Timeline, timeline::{
    DateDividerMode, TimelineBuilder, TimelineEventFocusThreadMode, TimelineFocus, TimelineReadReceiptTracking
}};
use ringbuf::{HeapProd, HeapRb, traits::{Consumer, Observer, Split}};
use tauri_plugin_updater::Update;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex as SyncMutex,
        atomic::{AtomicU64, Ordering},
    },
};

use shared::api::{UpdateStatus, events::LogEntry};
use tauri::{AppHandle, async_runtime::{Mutex, RwLock}};
use tokio::{sync::mpsc::{self, UnboundedReceiver, UnboundedSender}, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{TauriError, frontend::audio::emit_devices_update};

#[derive(Default)]
pub struct AppState {
    pub frontend_current_room_id: RwLock<Option<String>>,
    pub frontend_is_focused: RwLock<bool>,
    pub update: RwLock<Option<Update>>,
    pub update_bytes: RwLock<Option<Vec<u8>>>,
    pub update_status: RwLock<UpdateStatus>,
}

/// In-memory ring buffer of recent log lines. Every log record is pushed here
/// (regardless of whether the log window is open) so the window can show the
/// full backlog as soon as it opens. See `builder::add_logging_plugin`.
pub struct LogBuffer {
    entries: SyncMutex<VecDeque<LogEntry>>,
    seq: AtomicU64,
    capacity: usize,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: SyncMutex::new(VecDeque::with_capacity(capacity)),
            seq: AtomicU64::new(0),
            capacity,
        }
    }

    /// Assigns the next sequence number, stores the entry (evicting the oldest
    /// once at capacity) and returns the stored entry for emitting to the window.
    pub fn push(
        &self,
        level: String,
        timestamp: String,
        path: String,
        line: u32,
        message: String,
    ) -> LogEntry {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let entry = LogEntry {
            seq,
            level,
            timestamp,
            path,
            line,
            message,
        };
        if let Ok(mut entries) = self.entries.lock() {
            while entries.len() >= self.capacity {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }
        entry
    }

    /// Oldest-to-newest snapshot of the current buffer contents.
    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.entries
            .lock()
            .map(|entries| entries.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new(10_000)
    }
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

pub type LiveKitRoomManager = Arc<Mutex<HashMap<String, LiveKitRoomData>>>;

pub struct LiveKitRoomData {
    pub livekit_room: LiveKitRoom,
    pub cancellation_token: CancellationToken,
    pub key_index: i32,
    pub call_id: Uuid,
}

impl LiveKitRoomData {
    pub fn close_event_stream(&self) -> tokio_util::sync::WaitForCancellationFuture<'_> {
        self.cancellation_token.cancel();
        self.cancellation_token.cancelled()
    }
}

pub struct AudioManager {
    pub host: Arc<cpal::Host>,

    pub input_devices: Arc<SyncMutex<HashMap<DeviceId, cpal::Device>>>,
    pub output_devices: Arc<SyncMutex<HashMap<DeviceId, cpal::Device>>>,

    pub input_device: Arc<SyncMutex<Option<Device>>>,
    pub output_device: Arc<SyncMutex<Option<Device>>>,

    input_stream: SyncMutex<Option<Stream>>,
    output_stream: SyncMutex<Option<Stream>>,

    input_sender: UnboundedSender<Option<(UnboundedReceiver<f32>, SupportedStreamConfig)>>,
    pub output_producer: SyncMutex<Option<HeapProd<f32>>>,

    pub native_audio_source: Arc<SyncMutex<NativeAudioSource>>,
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

impl AudioManager {
    pub fn refresh_devices(&self, handle: AppHandle) -> Result<(), TauriError> {
        let (input_devices, output_devices) = get_devices(&self.host);

        *self.input_devices.lock().unwrap() = input_devices.clone();
        *self.output_devices.lock().unwrap() = output_devices.clone();

        let input_id = self
            .input_device
            .lock()
            .unwrap()
            .clone()
            .and_then(|d| d.id().ok());

        let active_input_id = match input_id {
            Some(id) => Some(id),
            None => self.host.default_input_device().and_then(|d| d.id().ok()),
        };

        if let Some(id) = &active_input_id
            && !input_devices.contains_key(id) {
                if let Some(device) = self.host.default_input_device() {
                    self.try_setup_input_stream_for_device(&device)?;
                } else {
                    *self.input_device.lock().unwrap() = None;
                    *self.input_stream.lock().unwrap() = None;
                }
            }

        let output_id = self
            .output_device
            .lock()
            .unwrap()
            .clone()
            .and_then(|d| d.id().ok());

        let active_output_id = match output_id {
            Some(id) => Some(id),
            None => self.host.default_output_device().and_then(|d| d.id().ok()),
        };

        if let Some(id) = &active_output_id
            && !output_devices.contains_key(id) {
                if let Some(device) = self.host.default_output_device() {
                    self.try_setup_output_stream_for_device(&device)?;
                } else {
                    *self.output_device.lock().unwrap() = None;
                    *self.output_stream.lock().unwrap() = None;
                    *self.output_producer.lock().unwrap() = None;
                }
            }

        let default_input_id = self
            .host
            .default_input_device()
            .and_then(|d| d.id().ok())
            .map(|id| id.to_string());

        let default_output_id = self
            .host
            .default_output_device()
            .and_then(|d| d.id().ok())
            .map(|id| id.to_string());

        emit_devices_update(input_devices, output_devices, default_input_id, default_output_id, active_input_id.map(|i| i.to_string()), active_output_id.map(|i| i.to_string()), handle);

        Ok(())
    }

    pub fn setup_device_refresh_loop(&self, handle: AppHandle) {
        let host = self.host.clone();
        let input_devices = self.input_devices.clone();
        let output_devices = self.output_devices.clone();
        let active_input = self.input_device.clone();
        let active_output = self.output_device.clone();

        std::thread::spawn(move || {
            let mut prev_input_ids: HashSet<DeviceId> = HashSet::new();
            let mut prev_output_ids: HashSet<DeviceId> = HashSet::new();
            let mut prev_active_input: Option<DeviceId> = None;
            let mut prev_active_output: Option<DeviceId> = None;

            loop {
                let (new_input_devices, new_output_devices) = get_devices(&host);

                let new_input_ids: HashSet<DeviceId> = new_input_devices.keys().cloned().collect();
                let new_output_ids: HashSet<DeviceId> = new_output_devices.keys().cloned().collect();
                let new_active_input = active_input.lock().unwrap().as_ref().and_then(|d| d.id().ok());
                let new_active_output = active_output.lock().unwrap().as_ref().and_then(|d| d.id().ok());

                if new_input_ids != prev_input_ids
                    || new_output_ids != prev_output_ids
                    || new_active_input != prev_active_input
                    || new_active_output != prev_active_output
                {
                    *input_devices.lock().unwrap() = new_input_devices.clone();
                    *output_devices.lock().unwrap() = new_output_devices.clone();

                    log::debug!("Audio devices changed");

                    emit_devices_update(
                        new_input_devices,
                        new_output_devices,
                        host.default_input_device().and_then(|d| d.id().ok()).map(|id| id.to_string()),
                        host.default_output_device().and_then(|d| d.id().ok()).map(|id| id.to_string()),
                        new_active_input.as_ref().map(|id| id.to_string()),
                        new_active_output.as_ref().map(|id| id.to_string()),
                        handle.clone(),
                    );

                    prev_input_ids = new_input_ids;
                    prev_output_ids = new_output_ids;
                    prev_active_input = new_active_input;
                    prev_active_output = new_active_output;
                }

                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });
    }

    pub fn new(handle: AppHandle) -> Self {
        let host = Arc::new(cpal::default_host());

        let (input_sender, input_receiver) = mpsc::unbounded_channel();
        let new = Self {
            host,
            input_sender,

            input_devices: Arc::new(SyncMutex::new(HashMap::new())),
            output_devices: Arc::new(SyncMutex::new(HashMap::new())),

            input_stream: SyncMutex::new(None),
            output_stream: SyncMutex::new(None),
            output_producer: SyncMutex::new(None),

            input_device: Arc::new(SyncMutex::new(None)),
            output_device: Arc::new(SyncMutex::new(None)),

            native_audio_source: Arc::new(SyncMutex::new(NativeAudioSource::new(
                AudioSourceOptions::default(),
                48_000,
                2,
                10,
            ))),
        };
        new.setup_global_input_handler(input_receiver);
        new.setup_device_refresh_loop(handle);
        new
    }

    pub fn try_setup_output_stream_for_device(&self, device: &Device) -> Result<(), TauriError> {
        let old_stream = self.output_stream.lock().unwrap().take();
        drop(old_stream);

        *self.output_device.lock().unwrap() = Some(device.clone());

        let config = device.default_output_config()?;

        let sample_rate = config.sample_rate();
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

        *self.output_stream.lock().unwrap() = Some(output_stream);
        *self.output_producer.lock().unwrap() = Some(producer);
        Ok(())
    }

    pub fn try_setup_input_stream_for_device(&self, device: &Device) -> Result<(), TauriError> {
        let old_stream = self.input_stream.lock().unwrap().take();
        let is_first_setup = old_stream.is_none();
        drop(old_stream);

        *self.input_device.lock().unwrap() = Some(device.clone());

        let (tx, rx) = mpsc::unbounded_channel();

        let config = device.default_input_config()?;
        let stream_config = config.config();

        if is_first_setup {
            // First setup: create the NativeAudioSource with the real device config.
            // On device switches we keep the existing source so any published LiveKit
            // track (which holds a clone of it) stays connected.
            *self.native_audio_source.lock().unwrap() = NativeAudioSource::new(
                AudioSourceOptions::default(),
                config.sample_rate(),
                config.channels() as u32,
                10,
            );
            log::debug!(
                "NativeAudioSource created: {}Hz {}ch",
                config.sample_rate(),
                config.channels()
            );
        } else {
            log::debug!(
                "Device switch: reusing existing NativeAudioSource ({}Hz {}ch)",
                config.sample_rate(),
                config.channels()
            );
        }

        let err_callback = |err: cpal::StreamError| {
            if err.to_string().contains("get_htstamp") {
                log::debug!("Mic stream timing quirk: {}", err);
            } else {
                log::error!("Mic stream error: {}", err);
            }
        };

        let input_stream = match config.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| for &sample in data { let _ = tx.send(sample); },
                err_callback, None
            )?,
            SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| for &sample in data { let _ = tx.send(sample as f32 / i16::MAX as f32); },
                err_callback, None
            )?,
            SampleFormat::U16 => device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| for &sample in data { let _ = tx.send((sample as f32 - 32768.0) / 32768.0); },
                err_callback, None
            )?,
            _ => return Err(TauriError::from("Unsupported sample format")),
        };

        input_stream.play()?;

        log::debug!(
            "Input stream playing; sending config ({}Hz, {}ch) to handler",
            config.sample_rate(),
            config.channels()
        );
        self.input_sender.send(Some((rx, config.clone())))?;
        *self.input_stream.lock().unwrap() = Some(input_stream);
        Ok(())
    }

    pub fn setup_global_input_handler(
        &self,
        mut input_data_receiver: UnboundedReceiver<
            Option<(UnboundedReceiver<f32>, SupportedStreamConfig)>,
        >,
    ) {
        let native_audio_source = self.native_audio_source.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build audio handler runtime");

            loop {
                let Some(next) = input_data_receiver.blocking_recv() else {
                    return;
                };
                let Some((mut data_receiver, config)) = next else {
                    return;
                };

                let sample_rate = config.sample_rate();
                let channels = config.channels() as u32;
                let samples_per_channel = (sample_rate / 100) as usize;
                let total_samples = samples_per_channel * channels as usize;

                // NativeAudioSource was already set with the correct config in
                // try_setup_input_stream_for_device — clone it once here so
                // capture_frame uses the same underlying source as the track.
                let source = native_audio_source.lock().unwrap().clone();
                log::debug!("Handler using NativeAudioSource for {}Hz {}ch stream", sample_rate, channels);

                let mut frame_buffer = Vec::with_capacity(total_samples);
                loop {
                    while frame_buffer.len() < total_samples {
                        if let Some(s) = data_receiver.blocking_recv() {
                            let scaled = (s * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32);
                            frame_buffer.push(scaled as i16);
                        } else {
                            break;
                        }
                    }

                    if frame_buffer.len() < total_samples {
                        break;
                    }

                    let frame = AudioFrame {
                        data: std::mem::take(&mut frame_buffer).into(),
                        sample_rate,
                        num_channels: channels,
                        samples_per_channel: samples_per_channel as u32,
                    };

                    if let Err(e) = rt.block_on(source.capture_frame(&frame)) {
                        log::error!("Failed to push audio frame: {e}");
                    }
                }
            }
        });
    }
}
