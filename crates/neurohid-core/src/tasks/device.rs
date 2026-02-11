//! # Device Task
//!
//! This task is responsible for connecting to data sources and streaming
//! samples to the signal processing task. It's the entry point for all brain
//! signal data in the system.
//!
//! ## Provider Selection
//!
//! The provider is selected based on `DeviceConfig::backend`:
//!
//! | Backend | Description                                        |
//! |---------|----------------------------------------------------|
//! | `Mock`  | Synthetic sine-wave generator (always available)   |
//! | `Lsl`   | Lab Streaming Layer — any LSL stream on the network|
//! | `Auto`  | Tries LSL first, then falls back to Mock           |
//!
//! ## Architecture
//!
//! The device task supports two modes:
//!
//! - **Interactive mode** (with `device_command_rx`): Scan+command loop that
//!   responds to `Rescan`, `Connect`, and `Disconnect` commands from the hub.
//!   Multiple streams can be connected simultaneously.
//!
//! - **Headless mode** (without `device_command_rx`): Auto-connects to the
//!   first discovered stream and streams until shutdown. Compatible with
//!   `cargo run -p neurohid-core`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{self, Duration};
use tokio_util::sync::CancellationToken;

use neurohid_device::mock::MockProvider;
use neurohid_device::{Device, DeviceProvider, MockDeviceConfig};
use neurohid_types::{
    config::{DeviceBackend, DeviceConfig},
    device::{DeviceId, DeviceInfo, DiscoveredStream},
    error::{DeviceError, Result},
    signal::Sample,
};

use crate::service::{DeviceCommand, ServiceState};

/// An active stream connection managed by the device task.
struct ActiveStream {
    cancel: CancellationToken,
    join_handle: tokio::task::JoinHandle<()>,
}

/// The device task connects to EEG devices and streams samples.
pub struct DeviceTask {
    config: DeviceConfig,
    sample_tx: mpsc::Sender<Sample>,
    state: Arc<RwLock<ServiceState>>,
    /// Optional channel to forward samples to calibration panel
    calibration_sample_tx: Option<mpsc::Sender<Sample>>,
    /// Atomic flag: when true, samples are also sent to calibration channel
    calibration_mode: Option<Arc<AtomicBool>>,
    /// Optional channel for receiving stream management commands from the hub
    device_command_rx: Option<mpsc::Receiver<DeviceCommand>>,
}

impl DeviceTask {
    /// Creates a new device task.
    pub fn new(
        config: DeviceConfig,
        sample_tx: mpsc::Sender<Sample>,
        state: Arc<RwLock<ServiceState>>,
        calibration_sample_tx: Option<mpsc::Sender<Sample>>,
        calibration_mode: Option<Arc<AtomicBool>>,
        device_command_rx: Option<mpsc::Receiver<DeviceCommand>>,
    ) -> Self {
        Self {
            config,
            sample_tx,
            state,
            calibration_sample_tx,
            calibration_mode,
            device_command_rx,
        }
    }

    /// Runs the device task until shutdown is signaled.
    pub async fn run(self, shutdown: broadcast::Receiver<()>) -> Result<()> {
        let provider = create_provider(&self.config).await?;

        if self.device_command_rx.is_some() {
            self.run_interactive(provider, shutdown).await
        } else {
            self.run_headless(provider, shutdown).await
        }
    }

    /// Interactive mode: scan+command loop with multi-stream management.
    async fn run_interactive(
        self,
        provider: Box<dyn DeviceProvider>,
        mut shutdown: broadcast::Receiver<()>,
    ) -> Result<()> {
        let mut command_rx = self.device_command_rx.unwrap();
        let mut active_streams: HashMap<String, ActiveStream> = HashMap::new();
        let mut rescan_interval = time::interval(Duration::from_secs(10));

        // Initial scan
        scan(&provider, &self.state, &active_streams).await;

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("Device task received shutdown signal");
                    break;
                }

                Some(cmd) = command_rx.recv() => {
                    match cmd {
                        DeviceCommand::Rescan => {
                            tracing::info!("Rescan requested");
                            scan(&provider, &self.state, &active_streams).await;
                        }

                        DeviceCommand::Connect(stream_id) => {
                            if active_streams.contains_key(&stream_id) {
                                tracing::warn!("Stream '{}' is already connected", stream_id);
                                continue;
                            }

                            tracing::info!("Connecting to stream '{}'", stream_id);

                            match provider
                                .connect(&DeviceId::new(&stream_id), None)
                                .await
                            {
                                Ok(device) => {
                                    let cancel = CancellationToken::new();
                                    let handle = spawn_stream_task(
                                        device,
                                        stream_id.clone(),
                                        self.sample_tx.clone(),
                                        self.calibration_sample_tx.clone(),
                                        self.calibration_mode.as_ref().map(Arc::clone),
                                        self.state.clone(),
                                        cancel.clone(),
                                    );

                                    active_streams.insert(
                                        stream_id.clone(),
                                        ActiveStream {
                                            cancel,
                                            join_handle: handle,
                                        },
                                    );

                                    // Update state after successful connect
                                    update_connection_state(
                                        &self.state,
                                        &active_streams,
                                    )
                                    .await;

                                    // Update the connected flag in-place instead
                                    // of a full re-scan (which would block on
                                    // another resolve_lsl call).
                                    set_stream_connected(&self.state, &stream_id, true).await;
                                }
                                Err(e) => {
                                    tracing::error!(
                                        "Failed to connect to stream '{}': {}",
                                        stream_id,
                                        e
                                    );
                                }
                            }
                        }

                        DeviceCommand::Disconnect(stream_id) => {
                            if let Some(active) = active_streams.remove(&stream_id) {
                                tracing::info!("Disconnecting stream '{}'", stream_id);
                                active.cancel.cancel();
                                // Wait for the task to finish (best-effort)
                                let _ = active.join_handle.await;

                                update_connection_state(
                                    &self.state,
                                    &active_streams,
                                )
                                .await;

                                // Update the connected flag in-place instead
                                // of a full re-scan.
                                set_stream_connected(&self.state, &stream_id, false).await;
                            } else {
                                tracing::warn!(
                                    "Stream '{}' is not connected, ignoring disconnect",
                                    stream_id
                                );
                            }
                        }
                    }
                }

                _ = rescan_interval.tick() => {
                    // Only rescan when no streams are connected or none have been found
                    let should_scan = active_streams.is_empty() || {
                        self.state.try_read()
                            .map(|st| st.discovered_streams.is_empty())
                            .unwrap_or(true)
                    };
                    if should_scan {
                        scan(&provider, &self.state, &active_streams).await;
                    }
                }
            }
        }

        // Clean up all active streams
        for (id, active) in active_streams.drain() {
            tracing::info!("Shutting down stream '{}'", id);
            active.cancel.cancel();
            let _ = active.join_handle.await;
        }

        {
            let mut state = self.state.write().await;
            state.device_connected = false;
            state.device_name = None;
        }

        Ok(())
    }

    /// Headless mode: auto-connect to first discovered stream (legacy behavior).
    async fn run_headless(
        self,
        provider: Box<dyn DeviceProvider>,
        mut shutdown: broadcast::Receiver<()>,
    ) -> Result<()> {
        // Discover available devices
        tracing::info!("Discovering devices...");
        let devices = provider.discover().await?;

        if devices.is_empty() {
            tracing::error!("No devices found");
            return Err(DeviceError::NoDeviceFound.into());
        }

        tracing::info!("Found {} device(s), connecting to first", devices.len());

        // Connect to the first device
        let mut device = provider.connect(&devices[0].id, None).await?;

        tracing::info!("Connected to device: {}", device.id());

        // Update shared state with device info
        {
            let mut state = self.state.write().await;
            state.device_connected = true;
            state.device_name = Some(device.id().to_string());
        }

        // Start streaming
        let mut stream = device.start_streaming().await?;

        tracing::info!("Streaming started");

        // Main loop: read samples and forward them to the signal task
        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("Device task received shutdown signal");
                    break;
                }

                sample_result = stream.next() => {
                    match sample_result {
                        Some(Ok(sample)) => {
                            // Update signal quality in shared state
                            if let Some(quality) = &sample.quality {
                                let avg_quality = quality.iter().sum::<f32>() / quality.len() as f32;
                                let mut state = self.state.write().await;
                                state.signal_quality = avg_quality;
                            }

                            // If calibration mode is active, fan-out the sample
                            if let (Some(flag), Some(tx)) = (&self.calibration_mode, &self.calibration_sample_tx) {
                                if flag.load(Ordering::Relaxed) {
                                    let _ = tx.try_send(sample.clone());
                                }
                            }

                            // Send sample to signal task
                            if self.sample_tx.send(sample).await.is_err() {
                                tracing::warn!("Sample receiver dropped, stopping device task");
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            tracing::warn!("Error reading sample: {}", e);
                        }
                        None => {
                            tracing::info!("Device stream ended");
                            break;
                        }
                    }
                }
            }
        }

        // Clean up
        tracing::info!("Stopping device streaming");
        {
            let mut state = self.state.write().await;
            state.device_connected = false;
            state.device_name = None;
        }
        device.stop_streaming().await?;
        device.disconnect().await?;

        Ok(())
    }
}

// ─── Stream Task ─────────────────────────────────────────────────────────────

/// Spawn a tokio task that streams samples from a single connected device.
///
/// The task runs until the `cancel` token is cancelled or the stream ends.
fn spawn_stream_task(
    mut device: Box<dyn Device>,
    stream_id: String,
    sample_tx: mpsc::Sender<Sample>,
    calibration_sample_tx: Option<mpsc::Sender<Sample>>,
    calibration_mode: Option<Arc<AtomicBool>>,
    state: Arc<RwLock<ServiceState>>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let stream_result = device.start_streaming().await;
        let mut sample_stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to start streaming for '{}': {}", stream_id, e);
                return;
            }
        };

        tracing::info!("Stream '{}' started", stream_id);

        // Poll device status periodically to pick up battery/quality updates
        let mut status_interval = time::interval(Duration::from_secs(5));
        // Read initial status immediately
        update_stream_status(&state, &stream_id, &*device).await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("Stream '{}' cancelled", stream_id);
                    break;
                }

                sample_result = sample_stream.next() => {
                    match sample_result {
                        Some(Ok(sample)) => {
                            // Update signal quality in shared state
                            if let Some(quality) = &sample.quality {
                                let avg_quality =
                                    quality.iter().sum::<f32>() / quality.len() as f32;
                                let mut st = state.write().await;
                                st.signal_quality = avg_quality;
                            }

                            // Fan-out to calibration channel if active
                            if let (Some(flag), Some(tx)) =
                                (&calibration_mode, &calibration_sample_tx)
                            {
                                if flag.load(Ordering::Relaxed) {
                                    let _ = tx.try_send(sample.clone());
                                }
                            }

                            // Send to signal processing pipeline
                            if sample_tx.send(sample).await.is_err() {
                                tracing::warn!(
                                    "Sample receiver dropped, stopping stream '{}'",
                                    stream_id
                                );
                                break;
                            }
                        }
                        Some(Err(e)) => {
                            tracing::warn!(
                                "Error reading sample from '{}': {}",
                                stream_id,
                                e
                            );
                        }
                        None => {
                            tracing::info!("Stream '{}' ended", stream_id);
                            break;
                        }
                    }
                }

                _ = status_interval.tick() => {
                    update_stream_status(&state, &stream_id, &*device).await;
                }
            }
        }

        // Best-effort cleanup
        let _ = device.stop_streaming().await;
        let _ = device.disconnect().await;
        tracing::info!("Stream task '{}' exited", stream_id);
    })
}

// ─── Scan / State Helpers ────────────────────────────────────────────────────

/// Read the device's current status and propagate battery/quality into the
/// matching `DiscoveredStream` entry in `ServiceState`. Also updates the
/// top-level `device_battery` field.
async fn update_stream_status(
    state: &Arc<RwLock<ServiceState>>,
    stream_id: &str,
    device: &dyn Device,
) {
    let status = device.status();
    let mut st = state.write().await;

    // Update the matching discovered stream entry
    if let Some(ds) = st.discovered_streams.iter_mut().find(|s| s.id == stream_id) {
        ds.battery_percent = status.battery_percent;
        ds.channel_quality = status.channel_quality.clone();
    }

    // Update top-level device_battery from any connected stream that reports it.
    // Use the first non-None battery value found.
    st.device_battery = st
        .discovered_streams
        .iter()
        .filter(|s| s.connected)
        .find_map(|s| s.battery_percent);
}

/// Scan for available streams and update `ServiceState::discovered_streams`.
async fn scan(
    provider: &Box<dyn DeviceProvider>,
    state: &Arc<RwLock<ServiceState>>,
    active_streams: &HashMap<String, ActiveStream>,
) {
    match provider.discover().await {
        Ok(devices) => {
            let mut discovered: Vec<DiscoveredStream> = devices
                .iter()
                .map(|info| device_info_to_discovered(info, active_streams))
                .collect();

            if discovered.is_empty() {
                tracing::info!("Scan found 0 streams (is a publisher running?)");
            } else {
                tracing::info!("Scan found {} stream(s)", discovered.len());
            }

            // Preserve battery/quality from the previous snapshot for connected
            // streams — the status polling task updates these, and a rescan
            // shouldn't erase them.
            let st = state.read().await;
            for ds in &mut discovered {
                if let Some(prev) = st.discovered_streams.iter().find(|p| p.id == ds.id) {
                    if ds.battery_percent.is_none() {
                        ds.battery_percent = prev.battery_percent;
                    }
                    if ds.channel_quality.is_none() {
                        ds.channel_quality = prev.channel_quality.clone();
                    }
                }
            }
            drop(st);

            let mut st = state.write().await;
            st.discovered_streams = discovered;
        }
        Err(e) => {
            tracing::warn!("Stream scan failed: {}", e);
        }
    }
}

/// Convert a `DeviceInfo` into a `DiscoveredStream`.
fn device_info_to_discovered(
    info: &DeviceInfo,
    active_streams: &HashMap<String, ActiveStream>,
) -> DiscoveredStream {
    let id = info.id.0.clone();
    let name = info.name.clone().unwrap_or_else(|| info.id.0.clone());

    let (stream_type, channel_count, sample_rate) = match &info.channel_config {
        Some(cfg) => {
            let type_str = match &info.device_type {
                neurohid_types::device::DeviceType::Unknown(s) => s.clone(),
                other => format!("{:?}", other),
            };
            (
                type_str,
                cfg.channels.len() as i32,
                cfg.sampling_rate_hz as f64,
            )
        }
        None => {
            let type_str = match &info.device_type {
                neurohid_types::device::DeviceType::Unknown(s) => s.clone(),
                other => format!("{:?}", other),
            };
            (type_str, 0, 0.0)
        }
    };

    let connected = active_streams.contains_key(&id);

    DiscoveredStream {
        id,
        name,
        stream_type,
        channel_count,
        sample_rate,
        connected,
        battery_percent: info.battery_percent,
        channel_quality: None,
        source_id: info.source_id.clone(),
    }
}

/// Update `device_connected` and `device_name` in shared state based on
/// the current set of active streams.
async fn update_connection_state(
    state: &Arc<RwLock<ServiceState>>,
    active_streams: &HashMap<String, ActiveStream>,
) {
    let mut st = state.write().await;
    st.device_connected = !active_streams.is_empty();
    if active_streams.is_empty() {
        st.device_name = None;
    } else {
        let names: Vec<&String> = active_streams.keys().collect();
        st.device_name = Some(
            names
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
}

/// Toggle the `connected` flag on a single discovered stream without
/// re-resolving the entire LSL network (which would add another full
/// `resolve_timeout_secs` of latency).
async fn set_stream_connected(state: &Arc<RwLock<ServiceState>>, stream_id: &str, connected: bool) {
    let mut st = state.write().await;
    if let Some(ds) = st.discovered_streams.iter_mut().find(|s| s.id == stream_id) {
        ds.connected = connected;
    }
}

// ─── Provider Factory ────────────────────────────────────────────────────────

/// Create a device provider based on the backend configuration.
///
/// For `Auto` mode, creates an `AutoProvider` that tries LSL on every
/// discovery call and falls back to Mock only when no LSL streams are found.
/// This avoids the old behaviour where a single 1-second check at startup
/// would permanently lock the provider to Mock.
async fn create_provider(config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    match config.backend {
        DeviceBackend::Mock => {
            tracing::info!("Using Mock device backend");
            Ok(Box::new(MockProvider::new(MockDeviceConfig::default())))
        }

        DeviceBackend::Lsl => {
            tracing::info!("Using LSL device backend");
            create_lsl_provider(config)
        }

        DeviceBackend::Auto => {
            tracing::info!("Using Auto device backend (LSL preferred, Mock fallback)");
            let lsl = create_lsl_provider(config)?;
            let mock = Box::new(MockProvider::new(MockDeviceConfig::default()));
            Ok(Box::new(AutoProvider::new(lsl, mock)))
        }
    }
}

/// Create an LSL device provider from the device configuration.
fn create_lsl_provider(config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    let lsl_config = config.lsl.clone().unwrap_or_default();
    Ok(Box::new(neurohid_device::LslProvider::new(lsl_config)))
}

// ─── Auto Provider ───────────────────────────────────────────────────────────

/// A composite provider that delegates to LSL first, falling back to Mock
/// on each `discover()` / `connect()` call instead of choosing once at startup.
struct AutoProvider {
    lsl: Box<dyn DeviceProvider>,
    mock: Box<dyn DeviceProvider>,
}

impl AutoProvider {
    fn new(lsl: Box<dyn DeviceProvider>, mock: Box<dyn DeviceProvider>) -> Self {
        Self { lsl, mock }
    }
}

#[async_trait::async_trait]
impl DeviceProvider for AutoProvider {
    fn device_type(&self) -> neurohid_types::device::DeviceType {
        neurohid_types::device::DeviceType::Unknown("Auto".into())
    }

    async fn is_available(&self) -> bool {
        // Always available — at minimum the mock layer works.
        true
    }

    async fn discover(&self) -> Result<Vec<neurohid_types::device::DeviceInfo>> {
        // Try LSL first; if it returns any streams, use those.
        match self.lsl.discover().await {
            Ok(devices) if !devices.is_empty() => {
                tracing::debug!(
                    "Auto: LSL discovered {} stream(s), using LSL",
                    devices.len()
                );
                Ok(devices)
            }
            Ok(_) => {
                tracing::debug!("Auto: no LSL streams found, falling back to Mock");
                self.mock.discover().await
            }
            Err(e) => {
                tracing::warn!("Auto: LSL discover failed ({e}), falling back to Mock");
                self.mock.discover().await
            }
        }
    }

    async fn connect(
        &self,
        device_id: &neurohid_types::device::DeviceId,
        settings: Option<neurohid_types::device::ConnectionSettings>,
    ) -> Result<Box<dyn Device>> {
        // Try LSL first. Mock device IDs start with "mock_" so we can
        // short-circuit, but to be safe we always attempt LSL first for
        // non-mock IDs.
        if device_id.0.starts_with("mock_") {
            return self.mock.connect(device_id, settings).await;
        }

        match self.lsl.connect(device_id, settings.clone()).await {
            Ok(device) => Ok(device),
            Err(e) => {
                tracing::warn!(
                    "Auto: LSL connect to '{}' failed ({e}), trying Mock",
                    device_id
                );
                self.mock.connect(device_id, settings).await
            }
        }
    }
}
