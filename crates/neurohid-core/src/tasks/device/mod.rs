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
//! | `Auto`  | Tries LSL first, then falls back to BrainFlow synthetic |
//! | `Serial`| Direct USB/serial adapter input                    |
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

mod discovery;
mod streaming;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt;
use tokio::sync::{RwLock, broadcast, mpsc};
use tokio::time::Duration;

use neurohid_device::DeviceProvider;
use neurohid_types::{
    config::DeviceConfig,
    device::DeviceId,
    error::Result,
    observability::{self as obs, EmitPolicyConfig, ObservabilityComponent, ObservabilityConfig},
    signal::Sample,
};

use crate::extension_registry::ExtensionRegistry;
use crate::observability::EmitGate;
use crate::service::{DeviceCommand, IntegrityStage, ServiceState};

use tokio::time;
use tokio_util::sync::CancellationToken;

use neurohid_types::error::DeviceError;

use discovery::{create_provider, scan};
use streaming::{
    ActiveStream, DEVICE_SUMMARY_EVERY_SAMPLES, DeviceSampleIntegrityTracker, StreamTaskContext,
    report_device_integrity_issue, spawn_stream_task,
};

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

/// Toggle the `connected` flag on a single discovered stream.
async fn set_stream_connected(state: &Arc<RwLock<ServiceState>>, stream_id: &str, connected: bool) {
    let mut st = state.write().await;
    if let Some(ds) = st.discovered_streams.iter_mut().find(|s| s.id == stream_id) {
        ds.connected = connected;
    }
}

/// The device task connects to EEG devices and streams samples.
pub struct DeviceTask {
    config: DeviceConfig,
    sample_tx: mpsc::Sender<Sample>,
    state: Arc<RwLock<ServiceState>>,
    calibration_sample_tx: Option<mpsc::Sender<Sample>>,
    calibration_mode: Option<Arc<AtomicBool>>,
    device_command_rx: Option<mpsc::Receiver<DeviceCommand>>,
    registry: Option<Arc<ExtensionRegistry>>,
    emit_gate: EmitGate,
    stream_emit_policy: EmitPolicyConfig,
}

impl DeviceTask {
    pub fn new(
        config: DeviceConfig,
        sample_tx: mpsc::Sender<Sample>,
        state: Arc<RwLock<ServiceState>>,
        calibration_sample_tx: Option<mpsc::Sender<Sample>>,
        calibration_mode: Option<Arc<AtomicBool>>,
        device_command_rx: Option<mpsc::Receiver<DeviceCommand>>,
        registry: Option<Arc<ExtensionRegistry>>,
        observability: ObservabilityConfig,
    ) -> Self {
        let policy = observability.policy_for(ObservabilityComponent::Device);
        Self {
            config,
            sample_tx,
            state,
            calibration_sample_tx,
            calibration_mode,
            device_command_rx,
            registry,
            emit_gate: EmitGate::new(policy.clone()),
            stream_emit_policy: policy,
        }
    }

    pub async fn run(mut self, shutdown: broadcast::Receiver<()>) -> Result<()> {
        tracing::info!(
            event = obs::event::TASK_STARTED,
            span = obs::span::DEVICE_RUN,
            stage = obs::stage::DEVICE,
            backend = ?self.config.backend,
            "Device task started"
        );
        {
            let mut state = self.state.write().await;
            state.set_stage_integrity_snapshot(IntegrityStage::Device, 0, false);
        }

        let provider = create_provider(&self.config, self.registry.as_deref()).await?;

        let result = if self.device_command_rx.is_some() {
            self.run_interactive(provider, shutdown).await
        } else {
            self.run_headless(provider, shutdown).await
        };

        if self.emit_gate.allow_info() {
            tracing::info!(
                event = obs::event::TASK_STOPPED,
                decision_id = obs::field::UNKNOWN,
                stream_id = obs::field::UNKNOWN,
                "Device task stopped"
            );
        }
        result
    }

    async fn run_interactive(
        &mut self,
        provider: Box<dyn DeviceProvider>,
        mut shutdown: broadcast::Receiver<()>,
    ) -> Result<()> {
        let Some(mut command_rx) = self.device_command_rx.take() else {
            tracing::warn!("Interactive device mode requested without command receiver");
            return Ok(());
        };
        let mut active_streams: HashMap<String, ActiveStream> = HashMap::new();
        let mut rescan_interval = time::interval(Duration::from_secs(10));

        let connected_ids: HashSet<String> = active_streams.keys().cloned().collect();
        scan(&*provider, &self.state, &connected_ids).await;

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
                            let connected_ids: HashSet<String> = active_streams.keys().cloned().collect();
                            scan(&*provider, &self.state, &connected_ids).await;
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
                                        StreamTaskContext {
                                            sample_tx: self.sample_tx.clone(),
                                            calibration_sample_tx: self.calibration_sample_tx.clone(),
                                            calibration_mode: self.calibration_mode.as_ref().map(Arc::clone),
                                            state: self.state.clone(),
                                            observability_policy: self.stream_emit_policy.clone(),
                                        },
                                        cancel.clone(),
                                    );

                                    active_streams.insert(
                                        stream_id.clone(),
                                        ActiveStream { cancel, join_handle: handle },
                                    );

                                    update_connection_state(&self.state, &active_streams).await;
                                    set_stream_connected(&self.state, &stream_id, true).await;
                                }
                                Err(e) => {
                                    tracing::error!("Failed to connect to stream '{}': {}", stream_id, e);
                                }
                            }
                        }

                        DeviceCommand::Disconnect(stream_id) => {
                            if let Some(active) = active_streams.remove(&stream_id) {
                                tracing::info!("Disconnecting stream '{}'", stream_id);
                                active.cancel.cancel();
                                let _ = active.join_handle.await;

                                update_connection_state(&self.state, &active_streams).await;
                                set_stream_connected(&self.state, &stream_id, false).await;
                            } else {
                                tracing::warn!("Stream '{}' is not connected, ignoring disconnect", stream_id);
                            }
                        }
                    }
                }

                _ = rescan_interval.tick() => {
                    let should_scan = active_streams.is_empty()
                        || self
                            .state
                            .try_read()
                            .map(|st| st.discovered_streams.is_empty())
                            .unwrap_or(true);
                    if should_scan {
                        let connected_ids: HashSet<String> = active_streams.keys().cloned().collect();
                        scan(&*provider, &self.state, &connected_ids).await;
                    }
                }
            }

            if self.emit_gate.allow_info() {
                tracing::info!(
                    event = obs::event::TASK_SUMMARY,
                    decision_id = obs::field::UNKNOWN,
                    stream_id = obs::field::UNKNOWN,
                    connected_streams = active_streams.len(),
                    discovered_streams = self
                        .state
                        .try_read()
                        .map(|state| state.discovered_streams.len())
                        .unwrap_or(0),
                    "Device task periodic summary"
                );
            }
        }

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

    async fn run_headless(
        &mut self,
        provider: Box<dyn DeviceProvider>,
        mut shutdown: broadcast::Receiver<()>,
    ) -> Result<()> {
        tracing::info!("Discovering devices...");
        let devices = provider.discover().await?;

        if devices.is_empty() {
            tracing::error!("No devices found");
            return Err(DeviceError::NoDeviceFound.into());
        }

        tracing::info!("Found {} device(s), connecting to first", devices.len());

        let mut device = provider.connect(&devices[0].id, None).await?;

        tracing::info!("Connected to device: {}", device.id());
        let stream_label = device.id().to_string();

        {
            let mut state = self.state.write().await;
            state.device_connected = true;
            state.device_name = Some(device.id().to_string());
        }

        let mut stream = device.start_streaming().await?;
        let mut integrity = DeviceSampleIntegrityTracker::new();

        tracing::info!("Streaming started");

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("Device task received shutdown signal");
                    break;
                }

                sample_result = stream.next() => {
                    match sample_result {
                        Some(Ok(sample)) => {
                            if let Some(issue) = integrity.observe_sample(&sample) {
                                report_device_integrity_issue(
                                    &self.state,
                                    &stream_label,
                                    issue,
                                    &mut self.emit_gate,
                                )
                                .await;
                            }

                            if let Some(quality) = &sample.quality {
                                let avg_quality = quality.iter().sum::<f32>() / quality.len() as f32;
                                let mut state = self.state.write().await;
                                state.signal_quality = avg_quality;
                            }

                            if let (Some(flag), Some(tx)) = (&self.calibration_mode, &self.calibration_sample_tx)
                                && flag.load(Ordering::Relaxed)
                            {
                                let _ = tx.try_send(sample.clone());
                            }

                            if self.sample_tx.send(sample).await.is_err() {
                                tracing::warn!("Sample receiver dropped, stopping device task");
                                break;
                            }

                            if integrity.samples_seen.is_multiple_of(DEVICE_SUMMARY_EVERY_SAMPLES)
                                && self.emit_gate.allow_info()
                            {
                                tracing::info!(
                                    event = obs::event::TASK_SUMMARY,
                                    decision_id = obs::field::UNKNOWN,
                                    stream_id = stream_label.as_str(),
                                    samples_seen = integrity.samples_seen,
                                    "Device stream periodic summary"
                                );
                            }
                        }
                        Some(Err(e)) => tracing::warn!("Error reading sample: {}", e),
                        None => {
                            tracing::info!("Device stream ended");
                            break;
                        }
                    }
                }
            }
        }

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
