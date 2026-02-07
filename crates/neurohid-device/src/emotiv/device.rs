//! # Emotiv Device
//!
//! Implements the [`Device`] trait for an Emotiv headset connected via
//! the Cortex API. Manages the streaming lifecycle by subscribing to
//! EEG and device quality data streams, converting them to NeuroHID
//! [`Sample`] values with contact quality populated.
//!
//! ## Streaming Architecture
//!
//! ```text
//! Cortex WebSocket (wss://localhost:6868)
//!   ↓ Reader loop (in CortexClient)
//!   ├── eeg events → eeg_rx channel
//!   └── dev events → dev_rx channel
//!         ↓
//! EEG Adapter Task             Dev Adapter Task
//!   ↓ parses EegEvent            ↓ parses DevEvent → DeviceQuality
//!   ↓ attaches latest quality    ↓ updates shared quality state
//!   ↓ converts to Sample
//! mpsc channel
//!   ↓ SampleStream (impl Stream)
//! NeuroHID pipeline
//! ```
//!
//! ## Key Improvement Over Previous Architecture
//!
//! The `CortexClient` is no longer consumed by `start_streaming()`. The
//! WebSocket is split into reader/writer halves, with the reader running
//! in a background task. This means:
//!
//! - `unsubscribe` is actually called during `stop_streaming()`
//! - `disconnect()` always performs clean session teardown
//! - Additional API calls (markers, records, profiles) can be made while streaming

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::Stream;
use tokio::sync::{mpsc, watch};

use neurohid_types::device::{
    ConnectionState, DeviceId, DeviceInfo, DeviceStatus, DeviceType,
};
use neurohid_types::error::{DeviceError, Result};
use neurohid_types::signal::DeviceChannelConfig;
use neurohid_types::now_micros;

use crate::traits::{Device, SampleStream};

use super::cortex_client::CortexClient;
use super::protocol::{
    BandPowerData, DetectionInfo, DetectionType, DeviceQuality, EegEvent, ExportFormat,
    FacialExpression, HeadsetInfo, MarkerInfo, MentalCommand, MotEvent, MotionData,
    PerformanceMetrics, ProfileAction, ProfileInfo, PowEvent, RecordInfo, Streams,
    TrainingStatus,
};

/// A connected Emotiv headset with an active Cortex session.
pub struct EmotivDevice {
    id: DeviceId,
    info: DeviceInfo,
    channel_config: DeviceChannelConfig,
    num_channels: usize,

    /// The Cortex WebSocket client. Retained during streaming (no longer consumed).
    client: CortexClient,
    session_id: String,
    cortex_token: String,

    // State
    connected: AtomicBool,
    streaming: Arc<AtomicBool>,

    /// Latest device quality from the "dev" stream, shared between
    /// the dev adapter task and the EEG adapter task.
    latest_quality: Arc<RwLock<Option<DeviceQuality>>>,

    // Status broadcasting
    status_tx: watch::Sender<DeviceStatus>,
    status_rx: watch::Receiver<DeviceStatus>,
}

impl EmotivDevice {
    /// Create a new EmotivDevice.
    ///
    /// Called by `EmotivProvider::connect()` after authentication and
    /// session creation are complete.
    pub(crate) fn new(
        id: DeviceId,
        device_type: DeviceType,
        channel_config: DeviceChannelConfig,
        client: CortexClient,
        session_id: String,
        cortex_token: String,
        headset: HeadsetInfo,
    ) -> Self {
        let num_channels = channel_config.channels.len();

        let info = DeviceInfo {
            id: id.clone(),
            device_type,
            name: Some(headset.id),
            firmware_version: headset.firmware,
            channel_config: Some(channel_config.clone()),
            battery_percent: None,
        };

        let initial_status = DeviceStatus {
            device_id: id.clone(),
            connection_state: ConnectionState::Connected,
            is_streaming: false,
            samples_received: 0,
            samples_dropped: 0,
            battery_percent: None,
            channel_quality: None,
            message: Some("Emotiv device ready".into()),
        };
        let (status_tx, status_rx) = watch::channel(initial_status);

        Self {
            id,
            info,
            channel_config,
            num_channels,
            client,
            session_id,
            cortex_token,
            connected: AtomicBool::new(true),
            streaming: Arc::new(AtomicBool::new(false)),
            latest_quality: Arc::new(RwLock::new(None)),
            status_tx,
            status_rx,
        }
    }

    fn update_status(&self) {
        // Read the latest quality info for status reporting
        let (battery_percent, channel_quality) = self
            .latest_quality
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().map(|q| {
                (Some(q.battery_percent), Some(q.channel_quality.clone()))
            }))
            .unwrap_or((None, None));

        let status = DeviceStatus {
            device_id: self.id.clone(),
            connection_state: if self.connected.load(Ordering::SeqCst) {
                ConnectionState::Connected
            } else {
                ConnectionState::Disconnected
            },
            is_streaming: self.streaming.load(Ordering::SeqCst),
            samples_received: 0,
            samples_dropped: 0,
            battery_percent,
            channel_quality,
            message: None,
        };
        let _ = self.status_tx.send(status);
    }

    /// Get a reference to the underlying Cortex client.
    ///
    /// This allows Emotiv-specific operations (markers, records, profiles,
    /// training) while the device is connected and possibly streaming.
    pub fn client(&self) -> &CortexClient {
        &self.client
    }

    /// Get the Cortex session ID.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get the Cortex auth token.
    pub fn cortex_token(&self) -> &str {
        &self.cortex_token
    }

    /// Get the latest device quality data from the "dev" stream.
    pub fn device_quality(&self) -> Option<DeviceQuality> {
        self.latest_quality.read().ok()?.clone()
    }

    // ─── Emotiv-Specific Stream Subscriptions ───────────────────────────
    //
    // These methods subscribe to additional Cortex data streams beyond the
    // core EEG+DEV streams used by `start_streaming()`. Each returns a
    // typed async Stream that yields parsed data.
    //
    // These can be called while streaming is active — the split WebSocket
    // architecture allows concurrent API calls and new subscriptions.

    /// Subscribe to the motion/IMU data stream.
    ///
    /// Returns a stream of [`MotionData`] containing accelerometer,
    /// magnetometer, and quaternion readings. Useful for motion-based
    /// artifact rejection (head movements correlate with EEG artifacts).
    ///
    /// Requires an active session (call after `start_streaming()` or
    /// at least after the provider has connected).
    pub async fn subscribe_motion(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = MotionData> + Send>>> {
        self.ensure_connected()?;

        let rx = self
            .client
            .add_stream_channel(Streams::MOT)
            .ok_or(DeviceError::CommunicationError(
                "Failed to create motion stream channel".into(),
            ))?;

        self.client
            .subscribe_streams(&self.cortex_token, &self.session_id, &[Streams::MOT])
            .await?;

        Ok(Box::pin(TypedStream::new(rx, |event| {
            let mot_event: MotEvent = serde_json::from_value(event).ok()?;
            MotionData::from_mot_array(&mot_event.mot, mot_event.time)
        })))
    }

    /// Subscribe to the band power stream.
    ///
    /// Returns a stream of [`BandPowerData`] containing per-channel
    /// frequency band powers (theta/alpha/betaL/betaH/gamma in µV²/Hz).
    /// Can cross-validate or supplement the signal pipeline's Welch PSD.
    pub async fn subscribe_band_power(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = BandPowerData> + Send>>> {
        self.ensure_connected()?;

        let rx = self
            .client
            .add_stream_channel(Streams::POW)
            .ok_or(DeviceError::CommunicationError(
                "Failed to create band power stream channel".into(),
            ))?;

        let num_channels = self.num_channels;

        Ok(Box::pin(TypedStream::new(rx, move |event| {
            let pow_event: PowEvent = serde_json::from_value(event).ok()?;
            BandPowerData::from_pow_array(&pow_event.pow, num_channels, pow_event.time)
        })))
    }

    /// Subscribe to the performance metrics stream.
    ///
    /// Returns a stream of [`PerformanceMetrics`] containing Emotiv's
    /// computed cognitive state metrics (engagement, stress, attention, etc.).
    /// Can be used as supplementary PPO features or calibration UI feedback.
    pub async fn subscribe_metrics(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = PerformanceMetrics> + Send>>> {
        self.ensure_connected()?;

        let rx = self
            .client
            .add_stream_channel(Streams::MET)
            .ok_or(DeviceError::CommunicationError(
                "Failed to create metrics stream channel".into(),
            ))?;

        Ok(Box::pin(TypedStream::new(rx, |event| {
            let met = event.get("met")?.as_array()?;
            // met array format: [eng, exc, lex, str, rel, int, foc, ...]
            // Some values may be null if signal quality is insufficient
            let f = |i: usize| -> Option<f32> {
                met.get(i).and_then(|v| v.as_f64()).map(|v| v as f32)
            };
            let time = event.get("time")?.as_f64()?;
            Some(PerformanceMetrics {
                timestamp: (time * 1_000_000.0) as i64,
                engagement: f(0),
                excitement: f(1),
                long_excitement: f(2),
                stress: f(3),
                relaxation: f(4),
                interest: f(5),
                attention: f(6),
                focus: f(7),
            })
        })))
    }

    /// Subscribe to the mental command stream.
    ///
    /// Returns a stream of [`MentalCommand`] with the detected action and power.
    /// Requires a loaded profile with trained mental commands (see
    /// `client().setup_profile()`).
    pub async fn subscribe_mental_commands(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = MentalCommand> + Send>>> {
        self.ensure_connected()?;

        let rx = self
            .client
            .add_stream_channel(Streams::COM)
            .ok_or(DeviceError::CommunicationError(
                "Failed to create mental command stream channel".into(),
            ))?;

        Ok(Box::pin(TypedStream::new(rx, |event| {
            let com = event.get("com")?.as_array()?;
            let action = com.first()?.as_str()?.to_string();
            let power = com.get(1)?.as_f64()? as f32;
            Some(MentalCommand { action, power })
        })))
    }

    /// Subscribe to the facial expression stream.
    ///
    /// Returns a stream of [`FacialExpression`] with eye actions,
    /// upper/lower face actions and their power levels.
    /// Eye blinks can serve as alternative HID input.
    pub async fn subscribe_facial_expressions(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = FacialExpression> + Send>>> {
        self.ensure_connected()?;

        let rx = self
            .client
            .add_stream_channel(Streams::FAC)
            .ok_or(DeviceError::CommunicationError(
                "Failed to create facial expression stream channel".into(),
            ))?;

        Ok(Box::pin(TypedStream::new(rx, |event| {
            let fac = event.get("fac")?.as_array()?;
            // fac array: [eyeAct, uAct, uPow, lAct, lPow]
            let eye_action = fac.first()?.as_str()?.to_string();
            let upper_face_action = fac.get(1)?.as_str()?.to_string();
            let upper_face_power = fac.get(2)?.as_f64()? as f32;
            let lower_face_action = fac.get(3)?.as_str()?.to_string();
            let lower_face_power = fac.get(4)?.as_f64()? as f32;
            Some(FacialExpression {
                eye_action,
                upper_face_action,
                upper_face_power,
                lower_face_action,
                lower_face_power,
            })
        })))
    }

    /// Unsubscribe from an additional stream that was added via one of
    /// the `subscribe_*` methods.
    pub async fn unsubscribe_stream(&self, stream: &str) -> Result<()> {
        self.client
            .unsubscribe_streams(&self.cortex_token, &self.session_id, &[stream])
            .await?;
        self.client.remove_stream_channel(stream);
        Ok(())
    }

    // ─── Markers ────────────────────────────────────────────────────────
    //
    // Time-stamped event markers for calibration trials and experiments.
    // Requires an active recording session (see `start_recording()`).

    /// Inject a time-stamped marker into the current session.
    ///
    /// Markers annotate specific events (e.g., stimulus onset, trial
    /// boundaries) during calibration or experiments. They can later
    /// be used to epoch the EEG data for analysis.
    ///
    /// # Arguments
    ///
    /// * `label` - A descriptive label for the event (e.g., "target_onset")
    /// * `value` - An integer value for programmatic identification
    /// * `time` - Optional Unix timestamp; if None, the current time is used
    pub async fn inject_marker(
        &self,
        label: &str,
        value: i32,
        time: Option<f64>,
    ) -> Result<MarkerInfo> {
        self.ensure_connected()?;
        self.client
            .inject_marker(&self.cortex_token, &self.session_id, label, value, time)
            .await
    }

    /// Update a marker to convert it from an instance marker to an interval.
    ///
    /// Call this after `inject_marker()` to set the end time of the event,
    /// turning a point-in-time marker into a duration marker.
    pub async fn update_marker(&self, marker_id: &str, time: Option<f64>) -> Result<()> {
        self.ensure_connected()?;
        self.client
            .update_marker(&self.cortex_token, &self.session_id, marker_id, time)
            .await
    }

    // ─── Records ────────────────────────────────────────────────────────
    //
    // Record calibration sessions for offline analysis and model retraining.

    /// Start a new recording.
    ///
    /// The recording captures all subscribed data streams into Emotiv's
    /// cloud storage. Can be exported later via `export_record()`.
    pub async fn start_recording(&self, title: &str) -> Result<RecordInfo> {
        self.ensure_connected()?;
        self.client
            .create_record(&self.cortex_token, &self.session_id, title)
            .await
    }

    /// Stop the current recording.
    pub async fn stop_recording(&self) -> Result<RecordInfo> {
        self.ensure_connected()?;
        self.client
            .stop_record(&self.cortex_token, &self.session_id)
            .await
    }

    /// Query recorded sessions.
    pub async fn query_records(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<RecordInfo>> {
        self.ensure_connected()?;
        self.client
            .query_records(&self.cortex_token, limit, offset)
            .await
    }

    /// Export recorded data to a file.
    ///
    /// # Arguments
    ///
    /// * `record_ids` - UUIDs of the records to export
    /// * `folder` - Local directory path for the export files
    /// * `format` - Export format (CSV or EDF)
    pub async fn export_record(
        &self,
        record_ids: &[String],
        folder: &str,
        format: ExportFormat,
    ) -> Result<()> {
        self.ensure_connected()?;
        self.client
            .export_record(&self.cortex_token, record_ids, folder, format)
            .await
    }

    // ─── Profiles ───────────────────────────────────────────────────────
    //
    // Emotiv profiles store trained mental command / facial expression
    // models. Loading a profile is required for `com` and `fac` streams.

    /// List all profiles for the current user.
    pub async fn query_profiles(&self) -> Result<Vec<ProfileInfo>> {
        self.ensure_connected()?;
        self.client
            .query_profiles(&self.cortex_token)
            .await
    }

    /// Get the profile currently loaded for this headset.
    pub async fn get_current_profile(&self) -> Result<Option<ProfileInfo>> {
        self.ensure_connected()?;
        self.client
            .get_current_profile(&self.cortex_token, &self.id.0)
            .await
    }

    /// Manage a profile: create, load, unload, save, rename, or delete.
    ///
    /// Loading a profile is required before `subscribe_mental_commands()`
    /// or `subscribe_facial_expressions()` will return meaningful data.
    pub async fn setup_profile(
        &self,
        profile_name: &str,
        action: ProfileAction,
    ) -> Result<()> {
        self.ensure_connected()?;
        self.client
            .setup_profile(&self.cortex_token, &self.id.0, profile_name, action)
            .await
    }

    // ─── BCI / Training ──────────────────────────────────────────────────
    //
    // Emotiv's built-in mental command and facial expression detection.
    // These use Emotiv's own trained models, separate from NeuroHID's PPO
    // decoder. Can provide supplementary features or direct BCI input.

    /// Get detection info for mental commands or facial expressions.
    ///
    /// Returns the list of available actions, controls, and events
    /// for the specified detection type.
    pub async fn get_detection_info(
        &self,
        detection: DetectionType,
    ) -> Result<DetectionInfo> {
        self.ensure_connected()?;
        self.client.get_detection_info(detection).await
    }

    /// Control the training lifecycle for mental commands or facial expressions.
    ///
    /// # Arguments
    ///
    /// * `detection` - Which type of detection to train
    /// * `status` - The training action (start, accept, reject, reset, erase)
    /// * `action` - The action being trained (e.g., "push", "pull", "neutral")
    pub async fn training(
        &self,
        detection: DetectionType,
        status: TrainingStatus,
        action: &str,
    ) -> Result<serde_json::Value> {
        self.ensure_connected()?;
        self.client
            .training(&self.cortex_token, &self.session_id, detection, status, action)
            .await
    }

    /// Get or set the active mental command actions.
    ///
    /// Pass `Some(actions)` to set, or `None` to get the current list.
    pub async fn mental_command_active_action(
        &self,
        actions: Option<&[&str]>,
    ) -> Result<serde_json::Value> {
        self.ensure_connected()?;
        self.client
            .mental_command_active_action(&self.cortex_token, &self.session_id, actions)
            .await
    }

    /// Get or set the mental command action sensitivity values.
    ///
    /// Pass `Some(values)` to set sensitivities (one per active action),
    /// or `None` to get the current values.
    pub async fn mental_command_action_sensitivity(
        &self,
        values: Option<&[i32]>,
    ) -> Result<serde_json::Value> {
        self.ensure_connected()?;
        self.client
            .mental_command_action_sensitivity(&self.cortex_token, &self.session_id, values)
            .await
    }

    /// Get the mental command brain map visualization data.
    pub async fn mental_command_brain_map(&self) -> Result<serde_json::Value> {
        self.ensure_connected()?;
        self.client
            .mental_command_brain_map(&self.cortex_token, &self.session_id)
            .await
    }

    /// Get the mental command training threshold.
    pub async fn mental_command_training_threshold(&self) -> Result<serde_json::Value> {
        self.ensure_connected()?;
        self.client
            .mental_command_training_threshold(&self.cortex_token, &self.session_id)
            .await
    }

    // ─── Helpers ────────────────────────────────────────────────────────

    fn ensure_connected(&self) -> Result<()> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(DeviceError::NotConnected.into());
        }
        Ok(())
    }
}

#[async_trait]
impl Device for EmotivDevice {
    fn id(&self) -> &DeviceId {
        &self.id
    }

    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn channel_config(&self) -> &DeviceChannelConfig {
        &self.channel_config
    }

    fn status(&self) -> DeviceStatus {
        self.status_rx.borrow().clone()
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn is_streaming(&self) -> bool {
        self.streaming.load(Ordering::SeqCst)
    }

    async fn start_streaming(&mut self) -> Result<SampleStream> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(DeviceError::NotConnected.into());
        }
        if self.streaming.load(Ordering::SeqCst) {
            return Err(DeviceError::DeviceBusy.into());
        }

        // Create stream channels for EEG and device quality data.
        // The reader loop will start routing events to these channels.
        let streams = [Streams::EEG, Streams::DEV];
        let receivers = self.client.create_stream_channels(&streams);

        // Subscribe to both streams via the Cortex API
        self.client
            .subscribe_streams(&self.cortex_token, &self.session_id, &streams)
            .await?;

        self.streaming.store(true, Ordering::SeqCst);
        self.update_status();

        let (tx, rx) = mpsc::channel(1024);
        let streaming = Arc::clone(&self.streaming);
        let num_channels = self.num_channels;
        let latest_quality = Arc::clone(&self.latest_quality);
        let status_tx = self.status_tx.clone();
        let device_id = self.id.clone();

        // Spawn the device quality adapter task (reads "dev" events,
        // updates the shared quality state).
        if let Some(mut dev_rx) = receivers.dev_rx {
            let quality = Arc::clone(&latest_quality);
            let num_ch = num_channels;
            let streaming_flag = Arc::clone(&streaming);
            let dev_status_tx = status_tx.clone();
            let dev_device_id = device_id.clone();

            tokio::spawn(async move {
                while streaming_flag.load(Ordering::SeqCst) {
                    match dev_rx.recv().await {
                        Some(event) => {
                            if let Some(dev_array) = event.get("dev").and_then(|v| v.as_array()) {
                                let dev_values: Vec<serde_json::Value> =
                                    dev_array.to_vec();
                                if let Some(dq) =
                                    DeviceQuality::from_dev_array(&dev_values, num_ch)
                                {
                                    let battery = dq.battery_percent;
                                    let cq = dq.channel_quality.clone();
                                    if let Ok(mut guard) = quality.write() {
                                        *guard = Some(dq);
                                    }
                                    // Update device status with new quality info
                                    let _ = dev_status_tx.send(DeviceStatus {
                                        device_id: dev_device_id.clone(),
                                        connection_state: ConnectionState::Connected,
                                        is_streaming: true,
                                        samples_received: 0,
                                        samples_dropped: 0,
                                        battery_percent: Some(battery),
                                        channel_quality: Some(cq),
                                        message: None,
                                    });
                                }
                            }
                        }
                        None => break, // Channel closed
                    }
                }
                tracing::debug!("Emotiv dev quality task exiting");
            });
        }

        // Spawn the EEG adapter task (reads "eeg" events, converts to Samples
        // with quality data attached).
        if let Some(mut eeg_rx) = receivers.eeg_rx {
            let quality = Arc::clone(&latest_quality);

            tokio::spawn(async move {
                let mut sequence: u64 = 0;

                while streaming.load(Ordering::SeqCst) {
                    match eeg_rx.recv().await {
                        Some(event) => {
                            if let Ok(eeg_event) =
                                serde_json::from_value::<EegEvent>(event)
                            {
                                // Take only EEG channels, discard trailing marker
                                let values: Vec<f32> = eeg_event
                                    .eeg
                                    .iter()
                                    .take(num_channels)
                                    .map(|&v| v as f32)
                                    .collect();

                                // Attach the latest contact quality data
                                let sample_quality = quality
                                    .read()
                                    .ok()
                                    .and_then(|guard| {
                                        guard.as_ref().map(|q| q.channel_quality.clone())
                                    });

                                let sample = neurohid_types::signal::Sample {
                                    device_timestamp: Some(
                                        (eeg_event.time * 1_000_000.0) as i64,
                                    ),
                                    system_timestamp: now_micros(),
                                    sequence_number: Some(sequence),
                                    values,
                                    quality: sample_quality,
                                };
                                sequence += 1;

                                if tx.send(Ok(sample)).await.is_err() {
                                    break; // Receiver dropped
                                }
                            }
                        }
                        None => break, // Channel closed
                    }
                }

                tracing::debug!("Emotiv EEG streaming task exiting");
            });
        }

        // Wrap the mpsc receiver as a Stream
        let stream = EmotivStream { rx };
        Ok(Box::pin(stream))
    }

    async fn stop_streaming(&mut self) -> Result<()> {
        if !self.streaming.load(Ordering::SeqCst) {
            return Ok(());
        }

        self.streaming.store(false, Ordering::SeqCst);

        // Actually unsubscribe from the streams (now possible because
        // the client is not consumed by the streaming task!)
        let streams = [Streams::EEG, Streams::DEV];
        if let Err(e) = self
            .client
            .unsubscribe_streams(&self.cortex_token, &self.session_id, &streams)
            .await
        {
            tracing::warn!("Failed to unsubscribe from streams: {}", e);
        }

        // Clear the stream channels so the reader loop stops routing
        self.client.clear_stream_channels();

        self.update_status();
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        // Stop streaming first if active
        if self.streaming.load(Ordering::SeqCst) {
            self.stop_streaming().await?;
        }

        self.connected.store(false, Ordering::SeqCst);

        // Always close the session and disconnect (no more conditional guard)
        let _ = self
            .client
            .close_session(&self.cortex_token, &self.session_id)
            .await;
        let _ = self.client.disconnect().await;

        self.update_status();
        Ok(())
    }

    fn status_stream(&self) -> Pin<Box<dyn Stream<Item = DeviceStatus> + Send>> {
        let rx = self.status_rx.clone();
        Box::pin(futures::stream::unfold(rx, |mut rx| async move {
            rx.changed().await.ok()?;
            let val = rx.borrow_and_update().clone();
            Some((val, rx))
        }))
    }
}

/// Stream adapter wrapping an mpsc receiver for EEG samples.
struct EmotivStream {
    rx: mpsc::Receiver<Result<neurohid_types::signal::Sample>>,
}

impl Stream for EmotivStream {
    type Item = Result<neurohid_types::signal::Sample>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

/// Generic stream adapter that receives raw JSON events from an mpsc channel
/// and transforms them into typed values using a parser closure.
///
/// Events that fail to parse are silently skipped (they may be malformed
/// or from an incompatible Cortex API version). This is consistent with
/// how the EEG adapter task handles parse failures.
struct TypedStream<T, F>
where
    F: Fn(serde_json::Value) -> Option<T>,
{
    rx: mpsc::Receiver<serde_json::Value>,
    parser: F,
}

impl<T, F> TypedStream<T, F>
where
    F: Fn(serde_json::Value) -> Option<T>,
{
    fn new(rx: mpsc::Receiver<serde_json::Value>, parser: F) -> Self {
        Self { rx, parser }
    }
}

impl<T, F> Stream for TypedStream<T, F>
where
    T: Send,
    F: Fn(serde_json::Value) -> Option<T> + Unpin + Send,
{
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.rx.poll_recv(cx) {
                Poll::Ready(Some(event)) => {
                    if let Some(parsed) = (self.parser)(event) {
                        return Poll::Ready(Some(parsed));
                    }
                    // Parse failed — skip and try the next event
                    continue;
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}
