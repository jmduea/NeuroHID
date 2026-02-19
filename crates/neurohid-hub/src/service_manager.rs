//! # Service Manager
//!
//! Manages runtime lifecycle and control for the hub.
//! Supports both embedded runtime hosting and optional external runtime control.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, watch};

use neurohid_core::service::{DeviceCommand, NeuroHidService, ServiceHandle, SignalCommand};
use neurohid_ipc::{IpcClient, IpcConfig, IpcTransport, send_control_request_blocking};
use neurohid_storage::ProfileStore;
use neurohid_types::observability::{self as obs, EmitGate, ObservabilityComponent};
use neurohid_types::{
    IpcChannelV3, IpcEnvelopeV3, RuntimeEventV3, RuntimeEventsSubscribeV3,
    config::{
        FallbackPolicy, IpcMode, ServiceConfig, ServiceRuntimeMode, SignalConfig, SystemConfig,
    },
    control::{
        ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload, ControlSnapshot,
        TrainerSnapshot,
    },
    profile::ProfileId,
};

use crate::data_bus::DataBus;
use crate::state::ServiceSnapshot;

const EXTERNAL_SNAPSHOT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const EXTERNAL_EVENT_STREAM_STALE_TIMEOUT: Duration = Duration::from_secs(2);
const EXTERNAL_EVENT_RECONNECT_BASE: Duration = Duration::from_millis(250);
const EXTERNAL_EVENT_RECONNECT_MAX: Duration = Duration::from_secs(5);
const EXTERNAL_CONNECT_TIMEOUT_MS: u64 = 120;
const EXTERNAL_IO_TIMEOUT_MS: u64 = 250;

#[derive(Debug, Clone, Default)]
struct ExternalEventState {
    latest_snapshot: Option<ServiceSnapshot>,
    last_seq: Option<u64>,
    replay_miss: bool,
    stream_connected: bool,
    last_event_at: Option<Instant>,
    last_error: Option<String>,
}

/// Manages service/runtime lifecycle for the hub.
pub struct ServiceManager {
    handle: Option<ServiceHandle>,
    last_error: Option<String>,
    /// Whether the data bus has been connected to this handle's broadcast receivers.
    bus_connected: bool,
    /// Cached snapshot from the last successful read.
    cached_snapshot: ServiceSnapshot,
    runtime_mode: ServiceRuntimeMode,
    external_ipc_mode: IpcMode,
    external_ipc_endpoint: String,
    last_external_poll: Option<Instant>,
    external_event_state: Arc<StdMutex<ExternalEventState>>,
    external_event_task: Option<tokio::task::JoinHandle<()>>,
    external_event_stop_tx: Option<watch::Sender<bool>>,
    control_emit_gate: std::sync::Mutex<EmitGate>,
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceManager {
    pub fn new() -> Self {
        let service_defaults = ServiceConfig::default();
        Self {
            handle: None,
            last_error: None,
            bus_connected: false,
            cached_snapshot: ServiceSnapshot::default(),
            runtime_mode: service_defaults.runtime_mode,
            external_ipc_mode: service_defaults.ipc_mode,
            external_ipc_endpoint: service_defaults.ipc_endpoint,
            last_external_poll: None,
            external_event_state: Arc::new(StdMutex::new(ExternalEventState::default())),
            external_event_task: None,
            external_event_stop_tx: None,
            control_emit_gate: std::sync::Mutex::new(EmitGate::new(
                service_defaults
                    .observability
                    .policy_for(ObservabilityComponent::Control),
            )),
        }
    }

    /// Synchronize manager mode/endpoint from latest config.
    pub fn configure(&mut self, config: &SystemConfig) {
        let mut service_config = config.service.clone();
        for warning in service_config.apply_legacy_ipc_aliases() {
            tracing::warn!("{warning}");
        }
        let next_mode = config.service.runtime_mode.clone();
        let next_ipc_mode = service_config.ipc_mode;
        let next_ipc_endpoint = service_config.ipc_endpoint;

        if self.runtime_mode != next_mode {
            if self.runtime_mode == ServiceRuntimeMode::Embedded {
                self.stop_embedded();
            } else {
                self.cached_snapshot = ServiceSnapshot::default();
                self.stop_external_event_worker();
            }
            self.last_external_poll = None;
        }

        if self.external_ipc_endpoint != next_ipc_endpoint
            || self.external_ipc_mode != next_ipc_mode
        {
            self.external_ipc_endpoint = next_ipc_endpoint;
            self.external_ipc_mode = next_ipc_mode;
            self.last_external_poll = None;
            self.stop_external_event_worker();
            if self.runtime_mode == ServiceRuntimeMode::External {
                self.cached_snapshot = ServiceSnapshot::default();
            }
        }

        self.runtime_mode = next_mode;
        if let Ok(mut gate) = self.control_emit_gate.lock() {
            *gate = EmitGate::new(
                config
                    .service
                    .observability
                    .policy_for(ObservabilityComponent::Control),
            );
        }
    }

    /// Start the service/runtime.
    pub fn start(
        &mut self,
        runtime: &tokio::runtime::Runtime,
        config: SystemConfig,
        profile_store: Option<ProfileStore>,
        profile_id: Option<ProfileId>,
    ) {
        self.configure(&config);
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                self.stop_external_event_worker();
                self.start_embedded(runtime, config, profile_store, profile_id)
            }
            ServiceRuntimeMode::External => {
                self.stop_embedded();
                self.last_external_poll = None;
                self.start_external_event_worker(runtime);
                match self.send_control_request(ControlRequest::new(ControlCommand::Snapshot)) {
                    Ok(response) => match response.payload {
                        ControlResponsePayload::Snapshot { snapshot } => {
                            self.cached_snapshot = Self::snapshot_from_control(snapshot);
                            self.last_error = None;
                            tracing::info!(
                                endpoint = %self.control_endpoint_label(),
                                "Connected to external runtime control endpoint"
                            );
                        }
                        ControlResponsePayload::Error { message } => {
                            self.set_last_error(format!(
                                "External runtime error from {}: {}",
                                self.control_endpoint_label(),
                                message
                            ));
                        }
                        ControlResponsePayload::Ack => {
                            self.set_last_error(format!(
                                "External runtime at {} returned unexpected ACK for snapshot",
                                self.control_endpoint_label()
                            ));
                        }
                        ControlResponsePayload::TrainerSnapshot { .. } => {
                            self.set_last_error(format!(
                                "External runtime at {} returned unexpected trainer snapshot for snapshot request",
                                self.control_endpoint_label()
                            ));
                        }
                    },
                    Err(error) => {
                        self.set_last_error(format!(
                            "Failed to reach external runtime at {}: {}",
                            self.control_endpoint_label(),
                            error
                        ));
                    }
                }
            }
        }
    }

    /// Stop the running service/runtime.
    pub fn stop(&mut self) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => self.stop_embedded(),
            ServiceRuntimeMode::External => {
                self.stop_embedded();
                self.stop_external_event_worker();
                match self.send_control_request(ControlRequest::new(ControlCommand::Shutdown)) {
                    Ok(response) => match response.payload {
                        ControlResponsePayload::Ack => {
                            self.cached_snapshot.running = false;
                            self.cached_snapshot.device_connected = false;
                            self.cached_snapshot.ipc_connected = false;
                            self.cached_snapshot.discovered_streams.clear();
                            self.last_external_poll = None;
                            self.last_error = None;
                        }
                        ControlResponsePayload::Error { message } => {
                            self.set_last_error(format!(
                                "External runtime shutdown request failed: {}",
                                message
                            ));
                        }
                        ControlResponsePayload::Snapshot { .. } => {
                            self.cached_snapshot.running = false;
                            self.last_external_poll = None;
                        }
                        ControlResponsePayload::TrainerSnapshot { .. } => {
                            self.set_last_error(
                                "External runtime returned unexpected trainer snapshot for shutdown".to_string(),
                            );
                        }
                    },
                    Err(error) => {
                        self.set_last_error(format!(
                            "Failed to request shutdown from external runtime at {}: {}",
                            self.control_endpoint_label(),
                            error
                        ));
                    }
                }
            }
        }
    }

    /// Synchronize the data bus with runtime broadcast channels.
    /// Only available in embedded mode.
    pub fn sync_data_bus(&mut self, bus: &mut DataBus) {
        if self.runtime_mode != ServiceRuntimeMode::Embedded {
            if self.bus_connected {
                bus.disconnect();
                self.bus_connected = false;
            }
            return;
        }

        if let Some(handle) = &self.handle {
            // Check if service is still active
            let active = handle.state.try_read().map(|s| s.active).unwrap_or(true);

            if active && !self.bus_connected {
                // Create new receivers by resubscribing from the existing ones
                let sample_rx = handle.sample_broadcast_rx.resubscribe();
                let feature_rx = handle.feature_broadcast_rx.resubscribe();
                let action_rx = handle.action_broadcast_rx.resubscribe();
                let marker_rx = handle.marker_broadcast_rx.resubscribe();
                bus.connect(sample_rx, feature_rx, action_rx, marker_rx);
                self.bus_connected = true;
                tracing::info!("Data bus connected to service broadcasts");
            } else if !active && self.bus_connected {
                bus.disconnect();
                self.bus_connected = false;
                tracing::info!("Data bus disconnected (service stopped)");
            }
        } else if self.bus_connected {
            bus.disconnect();
            self.bus_connected = false;
        }
    }

    /// Take a non-blocking snapshot of runtime state.
    pub fn snapshot(&mut self) -> ServiceSnapshot {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => self.snapshot_embedded(),
            ServiceRuntimeMode::External => self.snapshot_external(),
        }
    }

    /// Enter calibration mode (pauses HID emission, enables sample forwarding).
    pub fn enter_calibration_mode(&self) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    handle.set_calibration_mode(true);
                    tracing::info!("Entered calibration mode");
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::SetCalibrationMode { enabled: true });
            }
        }
    }

    /// Exit calibration mode (resumes normal HID emission).
    pub fn exit_calibration_mode(&self) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    handle.set_calibration_mode(false);
                    tracing::info!("Exited calibration mode");
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::SetCalibrationMode { enabled: false });
            }
        }
    }

    /// Enable or pause HID output.
    pub fn set_output_enabled(&self, enabled: bool) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    handle.set_output_enabled(enabled);
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::SetOutputEnabled { enabled });
            }
        }
    }

    /// Request decoder model reload for the active profile.
    pub fn reload_model(&self) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    handle.reload_model();
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::ReloadModel);
            }
        }
    }

    /// Request guarded candidate model promotion for the active profile.
    pub fn promote_candidate_model(&self) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    handle.promote_candidate_model();
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::PromoteCandidateModel);
            }
        }
    }

    /// Enable or disable runtime learning.
    pub fn set_learning_enabled(&self, enabled: bool) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    handle.set_learning_enabled(enabled);
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::SetLearningEnabled { enabled });
            }
        }
    }

    /// Trigger an ML bridge reconnect attempt.
    pub fn ml_bridge_reconnect(&self) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    handle.ml_bridge_reconnect();
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::MlBridgeReconnect);
            }
        }
    }

    /// Push fallback policy settings into the running runtime.
    pub fn set_fallback_policy(&self, policy: FallbackPolicy) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    handle.set_fallback_policy(policy);
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::SetFallbackPolicy { policy });
            }
        }
    }

    /// Query the trainer-side snapshot exposed by the runtime.
    pub fn trainer_snapshot(&mut self) -> Option<TrainerSnapshot> {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                let handle = self.handle.as_ref()?;
                let state = handle.state.try_read().ok()?;
                Some(TrainerSnapshot {
                    trainer_connected: state.ml_bridge_connected,
                    trainer_state: if state.ml_bridge_stalled {
                        "stalled".to_string()
                    } else if state.ml_bridge_connected {
                        "connected".to_string()
                    } else {
                        "disconnected".to_string()
                    },
                    replay_size: state.trainer_replay_size.unwrap_or(0),
                    training_step: state.trainer_step.unwrap_or(0),
                    last_heartbeat_us: state.ml_bridge_last_heartbeat_us,
                    last_error: state
                        .trainer_last_error
                        .clone()
                        .or_else(|| state.task_error.clone().map(|(_, error)| error)),
                    protocol_version: state.ml_protocol_version,
                })
            }
            ServiceRuntimeMode::External => {
                let endpoint = self.control_endpoint_label();
                match self
                    .send_control_request(ControlRequest::new(ControlCommand::TrainerSnapshot))
                {
                    Ok(response) => match response.payload {
                        ControlResponsePayload::TrainerSnapshot { snapshot } => Some(snapshot),
                        ControlResponsePayload::Error { message } => {
                            self.set_last_error(format!(
                                "External runtime trainer snapshot failed from {}: {}",
                                endpoint, message
                            ));
                            None
                        }
                        ControlResponsePayload::Ack | ControlResponsePayload::Snapshot { .. } => {
                            self.set_last_error(format!(
                                "External runtime at {} returned unexpected payload for trainer snapshot",
                                endpoint
                            ));
                            None
                        }
                    },
                    Err(error) => {
                        self.set_last_error(format!(
                            "Failed to query trainer snapshot from external runtime at {}: {}",
                            endpoint, error
                        ));
                        None
                    }
                }
            }
        }
    }

    /// Update active profile status used by runtime action gating.
    pub fn set_active_profile(
        &self,
        profile_id: Option<ProfileId>,
        profile_name: String,
        profile_ready: bool,
    ) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    handle.set_profile_status(profile_id, Some(profile_name), profile_ready);
                }
            }
            ServiceRuntimeMode::External => {
                let _ = profile_id;
                let _ = profile_name;
                let _ = profile_ready;
                tracing::debug!(
                    "External runtime mode does not currently support profile metadata updates"
                );
            }
        }
    }

    /// Whether the service is currently running.
    #[allow(dead_code)] // public API for future use
    pub fn is_running(&self) -> bool {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => self.handle.is_some(),
            ServiceRuntimeMode::External => self.cached_snapshot.running,
        }
    }

    /// Last error encountered during start/snapshot.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Request a rescan of available LSL streams.
    pub fn rescan_streams(&self) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    let _ = handle.device_command_tx.try_send(DeviceCommand::Rescan);
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::RescanStreams);
            }
        }
    }

    /// Connect to a specific stream by its id.
    pub fn connect_stream(&self, stream_id: &str) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    let _ = handle
                        .device_command_tx
                        .try_send(DeviceCommand::Connect(stream_id.to_string()));
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::ConnectStream {
                    stream_id: stream_id.to_string(),
                });
            }
        }
    }

    /// Disconnect from a specific stream by its id.
    pub fn disconnect_stream(&self, stream_id: &str) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    let _ = handle
                        .device_command_tx
                        .try_send(DeviceCommand::Disconnect(stream_id.to_string()));
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::DisconnectStream {
                    stream_id: stream_id.to_string(),
                });
            }
        }
    }

    /// Connect to all streams in a group (e.g., all streams from one device).
    pub fn connect_streams(&self, stream_ids: &[&str]) {
        for id in stream_ids {
            self.connect_stream(id);
        }
    }

    /// Disconnect from all streams in a group.
    pub fn disconnect_streams(&self, stream_ids: &[&str]) {
        for id in stream_ids {
            self.disconnect_stream(id);
        }
    }

    /// Push a live signal configuration update into the running signal task.
    pub fn update_signal_config(&self, cfg: SignalConfig) {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => {
                if let Some(handle) = &self.handle {
                    let _ = handle
                        .signal_command_tx
                        .try_send(SignalCommand::UpdateConfig(cfg));
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::SetSignalConfig { signal: cfg });
            }
        }
    }

    fn start_embedded(
        &mut self,
        runtime: &tokio::runtime::Runtime,
        config: SystemConfig,
        profile_store: Option<ProfileStore>,
        profile_id: Option<ProfileId>,
    ) {
        // If we have a handle but the service has stopped itself (e.g., task failure),
        // drop the stale handle so the user can restart without clicking "Stop" first.
        // Use `try_read()` first (non-blocking), but if the lock is contended fall
        // back to assuming the handle is stale rather than silently blocking restart.
        let should_drop_handle =
            self.handle
                .as_ref()
                .is_some_and(|handle| match handle.state.try_read() {
                    Ok(state) => !state.active,
                    // Lock contended — check if the join handle has already finished,
                    // which is a reliable signal that the service is dead.
                    Err(_) => handle.join_handle.is_finished(),
                });
        if should_drop_handle {
            self.handle.take();
            self.bus_connected = false;
        }

        if self.handle.is_some() {
            tracing::warn!("Service already running");
            return;
        }

        let (shutdown_tx, shutdown_rx) = broadcast::channel::<()>(1);

        let service_result = runtime.block_on(async {
            NeuroHidService::new(config, profile_store, profile_id, shutdown_rx).await
        });

        match service_result {
            Ok(service) => {
                let _guard = runtime.enter();
                let handle = service.spawn(shutdown_tx);
                self.handle = Some(handle);
                self.last_error = None;
                self.bus_connected = false;
                tracing::info!("Service started");
            }
            Err(e) => {
                self.set_last_error(format!("Failed to create service: {}", e));
            }
        }
    }

    fn stop_embedded(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.shutdown_tx.send(());
            self.bus_connected = false;
            tracing::info!("Service stop signal sent");
        }
    }

    fn snapshot_embedded(&mut self) -> ServiceSnapshot {
        let Some(handle) = &self.handle else {
            return ServiceSnapshot::default();
        };

        // try_read() is non-blocking — if the lock is held by a task,
        // return the cached snapshot (stale by at most one frame) to avoid
        // discarding discovered_streams and breaking quality routing.
        let state_guard = match handle.state.try_read() {
            Ok(guard) => guard,
            Err(_) => return self.cached_snapshot.clone(),
        };

        let uptime_secs = state_guard
            .started_at
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);

        let snap = ServiceSnapshot {
            running: state_guard.active,
            device_connected: state_guard.device_connected,
            device_name: state_guard.device_name.clone(),
            device_battery: state_guard.device_battery,
            signal_quality: state_guard.signal_quality,
            actions_emitted: state_guard.actions_emitted,
            errors_detected: state_guard.errors_detected,
            uptime_secs,
            ipc_connected: state_guard.ipc_connected,
            ipc_simulated: state_guard.ipc_simulated,
            learning_enabled: state_guard.learning_enabled,
            ml_bridge_connected: state_guard.ml_bridge_connected,
            ml_bridge_stalled: state_guard.ml_bridge_stalled,
            runtime_mode_state: state_guard.runtime_mode_state,
            enabled_capabilities: state_guard.enabled_capabilities.clone(),
            limited_capabilities_message: state_guard.limited_capabilities_message.clone(),
            fallback_model_kind: state_guard.fallback_model_kind.clone(),
            trainer_replay_size: state_guard.trainer_replay_size,
            trainer_step: state_guard.trainer_step,
            trainer_policy_loss: state_guard.trainer_policy_loss,
            trainer_value_loss: state_guard.trainer_value_loss,
            trainer_entropy: state_guard.trainer_entropy,
            trainer_last_error: state_guard.trainer_last_error.clone(),
            candidate_promotions_succeeded: state_guard.candidate_promotions_succeeded,
            candidate_promotions_rejected: state_guard.candidate_promotions_rejected,
            candidate_last_outcome: state_guard.candidate_last_outcome.clone(),
            ml_protocol_version: state_guard.ml_protocol_version,
            calibration_mode: state_guard.calibration_mode,
            output_enabled: state_guard.output_enabled,
            profile_ready: state_guard.profile_ready,
            decoder_ready: state_guard.decoder_ready,
            decoder_model_version: state_guard.decoder_model_version.clone(),
            signal_latency_last_us: state_guard.signal_latency_last_us,
            signal_latency_p95_us: state_guard.signal_latency_p95_us,
            decode_latency_last_us: state_guard.decode_latency_last_us,
            decode_latency_p95_us: state_guard.decode_latency_p95_us,
            action_latency_last_us: state_guard.action_latency_last_us,
            action_latency_p95_us: state_guard.action_latency_p95_us,
            latency_degraded: state_guard.latency_degraded,
            latency_alert_message: state_guard.latency_alert_message.clone(),
            active_profile_name: state_guard.active_profile_name.clone(),
            task_error: state_guard.task_error.clone(),
            discovered_streams: state_guard.discovered_streams.clone(),
            routed_eeg_streams: state_guard.routed_eeg_streams,
            routed_motion_streams: state_guard.routed_motion_streams,
            routed_auxiliary_streams: state_guard.routed_auxiliary_streams,
            routed_unknown_streams: state_guard.routed_unknown_streams,
            pipeline_integrity_degraded: state_guard.pipeline_integrity_degraded,
            integrity_issue_count: state_guard.integrity_issue_count,
            stage_health_summary: state_guard.stage_health_summary.clone(),
        };
        self.cached_snapshot = snap.clone();
        snap
    }

    fn start_external_event_worker(&mut self, runtime: &tokio::runtime::Runtime) {
        self.stop_external_event_worker();

        let endpoint = self.control_endpoint_label();
        let config = self.external_control_ipc_config();
        let state = Arc::clone(&self.external_event_state);
        let (stop_tx, stop_rx) = watch::channel(false);
        self.external_event_stop_tx = Some(stop_tx);
        self.external_event_task = Some(runtime.spawn(async move {
            run_external_event_worker(config, state, stop_rx).await;
            tracing::info!(endpoint = %endpoint, "external runtime.events worker stopped");
        }));
    }

    fn stop_external_event_worker(&mut self) {
        if let Some(stop_tx) = self.external_event_stop_tx.take() {
            let _ = stop_tx.send(true);
        }
        if let Some(task) = self.external_event_task.take() {
            task.abort();
        }
        if let Ok(mut state) = self.external_event_state.lock() {
            state.stream_connected = false;
            state.last_event_at = None;
            state.last_error = None;
        }
    }

    fn apply_external_runtime_event(
        state: &Arc<StdMutex<ExternalEventState>>,
        seq: u64,
        event: RuntimeEventV3,
    ) {
        let Ok(mut stream_state) = state.lock() else {
            return;
        };
        stream_state.last_seq = Some(seq);
        stream_state.last_event_at = Some(Instant::now());
        stream_state.stream_connected = true;

        match event {
            RuntimeEventV3::Snapshot { snapshot } => {
                stream_state.latest_snapshot = Some(Self::snapshot_from_control(snapshot));
                stream_state.last_error = None;
            }
            RuntimeEventV3::TrainerSnapshot { snapshot } => {
                if let Some(cached) = stream_state.latest_snapshot.as_mut() {
                    cached.ml_bridge_connected = snapshot.trainer_connected;
                    cached.trainer_replay_size = Some(snapshot.replay_size);
                    cached.trainer_step = Some(snapshot.training_step);
                    cached.trainer_last_error = snapshot.last_error.clone();
                    cached.ml_protocol_version = snapshot.protocol_version;
                }
            }
            RuntimeEventV3::TrainerStatus { status } => {
                if let Some(cached) = stream_state.latest_snapshot.as_mut() {
                    cached.trainer_replay_size = Some(status.replay_size);
                    cached.trainer_step = Some(status.training_step);
                    cached.trainer_policy_loss = status.policy_loss;
                    cached.trainer_value_loss = status.value_loss;
                    cached.trainer_entropy = status.entropy;
                    cached.trainer_last_error = status.last_error.clone();
                    cached.ml_bridge_connected = status.state != "disconnected";
                    cached.ml_bridge_stalled = status.state == "stalled";
                }
            }
            RuntimeEventV3::RuntimeTelemetry { telemetry } => {
                if let Some(cached) = stream_state.latest_snapshot.as_mut() {
                    cached.signal_latency_p95_us = telemetry.signal_latency_p95_us;
                    cached.decode_latency_p95_us = telemetry.decode_latency_p95_us;
                    cached.action_latency_p95_us = telemetry.action_latency_p95_us;
                    cached.integrity_issue_count = telemetry.dropped_ml_messages;
                }
            }
            RuntimeEventV3::Lifecycle { state, detail, .. } => {
                if state == "replay_miss" {
                    stream_state.replay_miss = true;
                    stream_state.last_error = Some(format!(
                        "runtime.events replay_miss; fallback snapshot polling enabled: {detail}"
                    ));
                } else if state == "replay_resumed" {
                    stream_state.replay_miss = false;
                    stream_state.last_error = None;
                }
            }
            _ => {}
        }
    }

    fn snapshot_external(&mut self) -> ServiceSnapshot {
        let now = Instant::now();
        if let Ok(state) = self.external_event_state.lock() {
            let stream_fresh = state
                .last_event_at
                .is_some_and(|ts| now.duration_since(ts) <= EXTERNAL_EVENT_STREAM_STALE_TIMEOUT);
            if state.stream_connected
                && !state.replay_miss
                && stream_fresh
                && let Some(snapshot) = state.latest_snapshot.clone()
            {
                self.cached_snapshot = snapshot.clone();
                self.last_error = state.last_error.clone();
                return snapshot;
            }
        }

        if let Some(last_poll) = self.last_external_poll
            && now.duration_since(last_poll) < EXTERNAL_SNAPSHOT_POLL_INTERVAL
        {
            return self.cached_snapshot.clone();
        }
        self.last_external_poll = Some(now);

        let endpoint = self.control_endpoint_label();
        match self.send_control_request(ControlRequest::new(ControlCommand::Snapshot)) {
            Ok(response) => match response.payload {
                ControlResponsePayload::Snapshot { snapshot } => {
                    self.cached_snapshot = Self::snapshot_from_control(snapshot);
                    self.last_error = None;
                    if let Ok(mut state) = self.external_event_state.lock() {
                        state.latest_snapshot = Some(self.cached_snapshot.clone());
                        state.replay_miss = false;
                        state.last_error = None;
                    }
                }
                ControlResponsePayload::Error { message } => {
                    self.set_last_error(format!(
                        "External runtime error from {}: {}",
                        endpoint, message
                    ));
                    self.cached_snapshot = ServiceSnapshot {
                        task_error: Some(("control".to_string(), message)),
                        ..ServiceSnapshot::default()
                    };
                    if let Ok(mut state) = self.external_event_state.lock() {
                        state.last_error = self.last_error.clone();
                    }
                }
                ControlResponsePayload::Ack => {
                    self.set_last_error(format!(
                        "External runtime at {} returned unexpected ACK for snapshot",
                        endpoint
                    ));
                    self.cached_snapshot = ServiceSnapshot::default();
                    if let Ok(mut state) = self.external_event_state.lock() {
                        state.last_error = self.last_error.clone();
                    }
                }
                ControlResponsePayload::TrainerSnapshot { .. } => {
                    self.set_last_error(format!(
                        "External runtime at {} returned unexpected trainer snapshot payload",
                        endpoint
                    ));
                    self.cached_snapshot = ServiceSnapshot::default();
                    if let Ok(mut state) = self.external_event_state.lock() {
                        state.last_error = self.last_error.clone();
                    }
                }
            },
            Err(error) => {
                self.set_last_error(format!(
                    "Failed to reach external runtime at {}: {}",
                    endpoint, error
                ));
                self.cached_snapshot = ServiceSnapshot {
                    task_error: Some(("control".to_string(), error)),
                    ..ServiceSnapshot::default()
                };
                if let Ok(mut state) = self.external_event_state.lock() {
                    state.last_error = self.last_error.clone();
                }
            }
        }

        self.cached_snapshot.clone()
    }

    fn send_external_command(&self, command: ControlCommand) {
        let endpoint = self.control_endpoint_label();
        match self.send_control_request(ControlRequest::new(command)) {
            Ok(response) => match response.payload {
                ControlResponsePayload::Error { message } => {
                    tracing::warn!(
                        endpoint = %endpoint,
                        "External runtime rejected command: {}",
                        message
                    );
                }
                ControlResponsePayload::TrainerSnapshot { .. } => {
                    tracing::warn!(
                        endpoint = %endpoint,
                        "External runtime returned unexpected trainer snapshot payload"
                    );
                }
                ControlResponsePayload::Ack | ControlResponsePayload::Snapshot { .. } => {}
            },
            Err(error) => {
                tracing::warn!(endpoint = %endpoint, "Failed to send external runtime command: {}", error);
            }
        }
    }

    fn send_control_request(&self, request: ControlRequest) -> Result<ControlResponse, String> {
        let endpoint = self.control_endpoint_label();
        let request_id = request.request_id.clone();
        let command = Self::control_command_name(&request.command);
        let started = Instant::now();
        let _request_span = tracing::debug_span!(
            obs::span::CONTROL_REQUEST,
            stage = obs::stage::CONTROL,
            decision_id = obs::field::UNKNOWN,
            stream_id = obs::field::UNKNOWN,
            command,
            request_id = request_id.as_deref().unwrap_or("none")
        )
        .entered();

        if self.allow_control_debug() {
            tracing::debug!(
                event = obs::event::CONTROL_REQUEST_RECEIVED,
                endpoint = %endpoint,
                request_id = request_id.as_deref().unwrap_or("none"),
                decision_id = obs::field::UNKNOWN,
                stream_id = obs::field::UNKNOWN,
                command,
                mode = ?self.external_ipc_mode,
                "Sending external control request"
            );
        }

        let config = self.external_control_ipc_config();
        let response = send_control_request_blocking(config, request, "hub-control", 1)
            .map_err(|error| format!("external control request failed: {}", error));

        match &response {
            Ok(ok) => {
                if self.allow_control_debug() {
                    tracing::debug!(
                        event = obs::event::CONTROL_RESPONSE_SENT,
                        endpoint = %endpoint,
                        request_id = request_id.as_deref().unwrap_or("none"),
                        decision_id = obs::field::UNKNOWN,
                        stream_id = obs::field::UNKNOWN,
                        command,
                        payload = %Self::control_response_kind(&ok.payload),
                        duration_ms = started.elapsed().as_millis() as u64,
                        "Received external control response"
                    );
                }
            }
            Err(error) => tracing::warn!(
                endpoint = %endpoint,
                request_id = request_id.as_deref().unwrap_or("none"),
                decision_id = obs::field::UNKNOWN,
                stream_id = obs::field::UNKNOWN,
                command,
                duration_ms = started.elapsed().as_millis() as u64,
                "External control request failed: {}",
                error
            ),
        }

        response
    }

    fn allow_control_debug(&self) -> bool {
        self.control_emit_gate
            .lock()
            .map(|mut gate| gate.allow_debug())
            .unwrap_or(true)
    }

    fn external_control_ipc_config(&self) -> IpcConfig {
        let transport = match self.external_ipc_mode {
            IpcMode::LocalSocket => IpcTransport::LocalSocket,
            IpcMode::TcpLoopback => IpcTransport::TcpLoopback,
        };
        IpcConfig {
            transport,
            endpoint: self.external_ipc_endpoint.clone(),
            connect_timeout_ms: EXTERNAL_CONNECT_TIMEOUT_MS,
            send_timeout_ms: EXTERNAL_IO_TIMEOUT_MS,
            recv_timeout_ms: EXTERNAL_IO_TIMEOUT_MS,
            ..IpcConfig::default()
        }
    }

    fn control_endpoint_label(&self) -> String {
        self.external_ipc_endpoint.clone()
    }

    fn control_response_kind(payload: &ControlResponsePayload) -> &'static str {
        match payload {
            ControlResponsePayload::Ack => "ack",
            ControlResponsePayload::Snapshot { .. } => "snapshot",
            ControlResponsePayload::TrainerSnapshot { .. } => "trainer_snapshot",
            ControlResponsePayload::Error { .. } => "error",
        }
    }

    fn control_command_name(command: &ControlCommand) -> &'static str {
        match command {
            ControlCommand::Snapshot => "snapshot",
            ControlCommand::Shutdown => "shutdown",
            ControlCommand::SetCalibrationMode { .. } => "set_calibration_mode",
            ControlCommand::SetOutputEnabled { .. } => "set_output_enabled",
            ControlCommand::ReloadModel => "reload_model",
            ControlCommand::PromoteCandidateModel => "promote_candidate_model",
            ControlCommand::RescanStreams => "rescan_streams",
            ControlCommand::ConnectStream { .. } => "connect_stream",
            ControlCommand::DisconnectStream { .. } => "disconnect_stream",
            ControlCommand::SetLearningEnabled { .. } => "set_learning_enabled",
            ControlCommand::MlBridgeReconnect => "ml_bridge_reconnect",
            ControlCommand::TrainerSnapshot => "trainer_snapshot",
            ControlCommand::SetFallbackPolicy { .. } => "set_fallback_policy",
            ControlCommand::SetSignalConfig { .. } => "set_signal_config",
        }
    }

    fn snapshot_from_control(snapshot: ControlSnapshot) -> ServiceSnapshot {
        ServiceSnapshot {
            running: snapshot.running,
            device_name: snapshot.device_name,
            device_battery: snapshot.device_battery,
            device_connected: snapshot.device_connected,
            signal_quality: snapshot.signal_quality,
            actions_emitted: snapshot.actions_emitted,
            errors_detected: snapshot.errors_detected,
            uptime_secs: snapshot.uptime_secs,
            ipc_connected: snapshot.ipc_connected,
            ipc_simulated: snapshot.ipc_simulated,
            learning_enabled: snapshot.learning_enabled,
            ml_bridge_connected: snapshot.ml_bridge_connected,
            ml_bridge_stalled: snapshot.ml_bridge_stalled,
            runtime_mode_state: snapshot.runtime_mode_state,
            enabled_capabilities: snapshot.enabled_capabilities,
            limited_capabilities_message: snapshot.limited_capabilities_message,
            fallback_model_kind: snapshot.fallback_model_kind,
            trainer_replay_size: snapshot.trainer_replay_size,
            trainer_step: snapshot.trainer_step,
            trainer_policy_loss: snapshot.trainer_policy_loss,
            trainer_value_loss: snapshot.trainer_value_loss,
            trainer_entropy: snapshot.trainer_entropy,
            trainer_last_error: snapshot.trainer_last_error,
            candidate_promotions_succeeded: snapshot.candidate_promotions_succeeded,
            candidate_promotions_rejected: snapshot.candidate_promotions_rejected,
            candidate_last_outcome: snapshot.candidate_last_outcome,
            ml_protocol_version: snapshot.ml_protocol_version,
            calibration_mode: snapshot.calibration_mode,
            output_enabled: snapshot.output_enabled,
            profile_ready: snapshot.profile_ready,
            decoder_ready: snapshot.decoder_ready,
            decoder_model_version: snapshot.decoder_model_version,
            latency_degraded: snapshot.latency_degraded,
            latency_alert_message: snapshot.latency_alert_message,
            active_profile_name: snapshot.active_profile_name,
            signal_latency_last_us: snapshot.signal_latency_last_us,
            signal_latency_p95_us: snapshot.signal_latency_p95_us,
            decode_latency_last_us: snapshot.decode_latency_last_us,
            decode_latency_p95_us: snapshot.decode_latency_p95_us,
            action_latency_last_us: snapshot.action_latency_last_us,
            action_latency_p95_us: snapshot.action_latency_p95_us,
            task_error: snapshot.task_error,
            discovered_streams: snapshot.discovered_streams,
            routed_eeg_streams: snapshot.routed_eeg_streams,
            routed_motion_streams: snapshot.routed_motion_streams,
            routed_auxiliary_streams: snapshot.routed_auxiliary_streams,
            routed_unknown_streams: snapshot.routed_unknown_streams,
            pipeline_integrity_degraded: snapshot.pipeline_integrity_degraded,
            integrity_issue_count: snapshot.integrity_issue_count,
            stage_health_summary: snapshot.stage_health_summary,
        }
    }

    fn set_last_error(&mut self, message: String) {
        if self.last_error.as_deref() != Some(message.as_str()) {
            tracing::error!("{}", message);
        }
        self.last_error = Some(message);
    }
}

async fn run_external_event_worker(
    config: IpcConfig,
    state: Arc<StdMutex<ExternalEventState>>,
    mut stop_rx: watch::Receiver<bool>,
) {
    let mut reconnect_delay = EXTERNAL_EVENT_RECONNECT_BASE;
    loop {
        if *stop_rx.borrow() {
            break;
        }

        let resume_from_seq = state
            .lock()
            .ok()
            .and_then(|guard| guard.last_seq.map(|seq| seq.saturating_add(1)));

        let mut client = IpcClient::new(config.clone());
        if let Err(error) = client.connect().await {
            set_external_event_stream_error(&state, format!("connect failed: {error}"));
            if sleep_external_reconnect(reconnect_delay, &mut stop_rx).await {
                break;
            }
            reconnect_delay = (reconnect_delay * 2).min(EXTERNAL_EVENT_RECONNECT_MAX);
            continue;
        }

        let subscribe = RuntimeEventsSubscribeV3 {
            families: vec![
                "snapshot".to_string(),
                "trainer_snapshot".to_string(),
                "trainer_status".to_string(),
                "runtime_telemetry".to_string(),
                "decision_event".to_string(),
                "errp_window".to_string(),
                "errp_result".to_string(),
                "integrity_issue".to_string(),
                "lifecycle".to_string(),
            ],
            include_snapshot: true,
            include_capabilities: false,
            max_events: None,
            max_duration_ms: None,
            resume_from_seq,
            sample_every: 1,
            snapshot_interval_ms: 1_000,
        };
        let subscribe_envelope = match IpcEnvelopeV3::new(
            IpcChannelV3::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("hub-external-runtime-events".to_string()),
            &subscribe,
        ) {
            Ok(envelope) => envelope,
            Err(error) => {
                set_external_event_stream_error(
                    &state,
                    format!("failed to encode runtime.events subscribe envelope: {error}"),
                );
                let _ = client.disconnect().await;
                break;
            }
        };
        if let Err(error) = client.send(subscribe_envelope).await {
            set_external_event_stream_error(&state, format!("subscribe send failed: {error}"));
            let _ = client.disconnect().await;
            if sleep_external_reconnect(reconnect_delay, &mut stop_rx).await {
                break;
            }
            reconnect_delay = (reconnect_delay * 2).min(EXTERNAL_EVENT_RECONNECT_MAX);
            continue;
        }

        reconnect_delay = EXTERNAL_EVENT_RECONNECT_BASE;
        if let Ok(mut stream_state) = state.lock() {
            stream_state.stream_connected = true;
            stream_state.last_error = None;
        }

        loop {
            tokio::select! {
                changed = stop_rx.changed() => {
                    let stop_requested = changed.is_ok() && *stop_rx.borrow();
                    if stop_requested {
                        let _ = client.disconnect().await;
                        return;
                    }
                }
                incoming = client.recv() => {
                    let envelope = match incoming {
                        Ok(envelope) => envelope,
                        Err(error) => {
                            set_external_event_stream_error(
                                &state,
                                format!("stream receive failed: {error}"),
                            );
                            break;
                        }
                    };

                    if envelope.channel != IpcChannelV3::RuntimeEvents || envelope.msg_type != "event" {
                        continue;
                    }

                    let seq = envelope.seq;
                    match envelope.decode_payload::<RuntimeEventV3>() {
                        Ok(event) => ServiceManager::apply_external_runtime_event(&state, seq, event),
                        Err(error) => {
                            set_external_event_stream_error(
                                &state,
                                format!("runtime.events payload decode failed: {error}"),
                            );
                        }
                    }
                }
            }
        }

        let _ = client.disconnect().await;
        if sleep_external_reconnect(reconnect_delay, &mut stop_rx).await {
            break;
        }
        reconnect_delay = (reconnect_delay * 2).min(EXTERNAL_EVENT_RECONNECT_MAX);
    }
}

async fn sleep_external_reconnect(delay: Duration, stop_rx: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        changed = stop_rx.changed() => changed.is_ok() && *stop_rx.borrow(),
    }
}

fn set_external_event_stream_error(state: &Arc<StdMutex<ExternalEventState>>, message: String) {
    if let Ok(mut stream_state) = state.lock() {
        stream_state.stream_connected = false;
        stream_state.last_error = Some(message);
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::{Duration, Instant};

    use neurohid_core::tasks::TrainerIngressEvent;
    #[cfg(windows)]
    use neurohid_ipc::{HelloV2, RuntimeMlRoleV2, TrainerStreamKindV3};
    use neurohid_ipc::{IpcClient, IpcConfig, IpcTransport};
    use neurohid_types::{
        ControlRpcRequestV3, ControlRpcResponseV3, IPC_PROTOCOL_V3, IpcChannelV3, IpcEnvelopeV3,
        config::{ControlTransport, DeviceBackend, IpcMode, ServiceRuntimeMode, SystemConfig},
        control::{
            ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload,
            ControlSnapshot, RuntimeModeState,
        },
    };
    #[cfg(windows)]
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[cfg(windows)]
    use tokio::net::windows::named_pipe::ServerOptions;

    use super::ServiceManager;
    use crate::state::ServiceSnapshot;

    #[test]
    fn snapshot_tracks_real_ipc_connect_disconnect_transitions() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        let mut manager = ServiceManager::new();
        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::Mock;
        config.service.ipc_simulation_enabled = false;
        config.service.ipc_mode = IpcMode::TcpLoopback;
        config.service.ipc_endpoint = format!("127.0.0.1:{}", allocate_test_port());

        manager.start(&runtime, config.clone(), None, None);
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| snap.running);

        let initial = manager.snapshot();
        assert!(!initial.ipc_connected);
        assert!(!initial.ipc_simulated);

        let trainer_ingress = manager
            .handle
            .as_ref()
            .expect("embedded runtime handle should be available")
            .trainer_ingress_tx
            .clone();
        runtime.block_on(async {
            trainer_ingress
                .send(TrainerIngressEvent::Connected {
                    session_id: "hub-test-trainer".to_string(),
                })
                .await
                .expect("trainer connect event should send");
        });

        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            snap.ipc_connected && !snap.ipc_simulated
        });

        runtime.block_on(async {
            trainer_ingress
                .send(TrainerIngressEvent::Disconnected)
                .await
                .expect("trainer disconnect event should send");
        });

        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            !snap.ipc_connected && !snap.ipc_simulated
        });

        manager.stop();

        let stopped = manager.snapshot();
        assert!(!stopped.running);
        assert!(!stopped.ipc_connected);
        assert!(!stopped.ipc_simulated);
    }

    #[test]
    fn external_mode_routes_snapshot_and_commands() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        let control_port = allocate_test_port();
        let server_join = spawn_mock_control_server(control_port);

        let mut manager = ServiceManager::new();
        let mut config = SystemConfig::default();
        config.service.runtime_mode = ServiceRuntimeMode::External;
        config.service.control_transport = ControlTransport::TcpLoopback;
        config.service.control_host = "127.0.0.1".to_string();
        config.service.control_port = control_port;

        manager.configure(&config);
        manager.start(&runtime, config, None, None);

        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| snap.running);
        manager.enter_calibration_mode();
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            snap.calibration_mode
        });

        manager.set_output_enabled(false);
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            !snap.output_enabled
        });

        manager.update_signal_config(SystemConfig::default().signal);

        manager.stop();
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| !snap.running);

        server_join
            .join()
            .expect("mock control server thread should join");
    }

    #[test]
    fn snapshot_reports_simulated_bridge_with_explicit_tcp_override() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        let mut manager = ServiceManager::new();
        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::Mock;
        config.service.ipc_simulation_enabled = true;
        config.service.ipc_mode = IpcMode::TcpLoopback;
        config.service.ipc_endpoint = format!("127.0.0.1:{}", allocate_test_port());

        manager.start(&runtime, config, None, None);
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            snap.running && snap.ipc_connected && snap.ipc_simulated
        });
        manager.stop();
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| !snap.running);
    }

    #[cfg(windows)]
    #[test]
    fn snapshot_tracks_named_pipe_reconnect_and_stall_recovery() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        let mut manager = ServiceManager::new();
        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::Mock;
        config.service.ipc_simulation_enabled = false;
        config.service.ipc_mode = IpcMode::LocalSocket;
        config.service.ipc_endpoint = unique_pipe_name("neurohid_ipc_test");
        config.service.ml_stall_timeout_ms = 120;
        config.service.ml_heartbeat_interval_ms = 50;

        manager.start(&runtime, config.clone(), None, None);
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| snap.running);

        let mut client =
            runtime.block_on(connect_test_named_pipe_client(&config.service.ipc_endpoint));
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            snap.ipc_connected && snap.ml_bridge_connected
        });
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            snap.ml_bridge_stalled
        });

        runtime.block_on(async {
            let hello = HelloV2 {
                protocol: "neurohid_runtime_ml_v3".to_string(),
                role: RuntimeMlRoleV2::Trainer,
                capabilities: vec!["errp_result".to_string()],
                profile_id: None,
                feature_schema_version: None,
                action_schema_version: None,
                decoder_model_version: None,
                trainer_name: Some("test-trainer".to_string()),
                trainer_version: Some("0.0.0".to_string()),
            };
            let envelope = IpcEnvelopeV3::new(
                IpcChannelV3::TrainerStream,
                TrainerStreamKindV3::Hello.as_msg_type(),
                1,
                None,
                Some("named-pipe-test".to_string()),
                &hello,
            )
            .expect("hello envelope should encode");
            client
                .send(envelope)
                .await
                .expect("hello send should succeed");
        });
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            !snap.ml_bridge_stalled
        });

        runtime.block_on(async {
            client
                .disconnect()
                .await
                .expect("disconnect should succeed");
        });
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            !snap.ipc_connected
        });

        let mut reconnect_client =
            runtime.block_on(connect_test_named_pipe_client(&config.service.ipc_endpoint));
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            snap.ipc_connected
        });
        runtime.block_on(async {
            reconnect_client
                .disconnect()
                .await
                .expect("disconnect should succeed");
        });

        manager.stop();
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| !snap.running);
    }

    #[cfg(windows)]
    #[test]
    fn external_mode_routes_snapshot_and_commands_over_named_pipe() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");

        let pipe_name = unique_pipe_name("neurohid_control_test");
        let server_join = spawn_mock_named_pipe_control_server(pipe_name.clone());

        let mut manager = ServiceManager::new();
        let mut config = SystemConfig::default();
        config.service.runtime_mode = ServiceRuntimeMode::External;
        config.service.control_transport = ControlTransport::NamedPipe;
        config.service.control_pipe_name = pipe_name;

        manager.configure(&config);
        manager.start(&runtime, config, None, None);

        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| snap.running);
        manager.enter_calibration_mode();
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            snap.calibration_mode
        });

        manager.set_output_enabled(false);
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            !snap.output_enabled
        });

        manager.stop();
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| !snap.running);

        server_join
            .join()
            .expect("mock named-pipe control server should join");
    }

    fn wait_for_snapshot(
        manager: &mut ServiceManager,
        timeout: Duration,
        predicate: impl Fn(&ServiceSnapshot) -> bool,
    ) {
        let start = Instant::now();
        loop {
            let snap = manager.snapshot();
            if predicate(&snap) {
                return;
            }

            if start.elapsed() > timeout {
                panic!("snapshot did not reach expected state before timeout: {snap:?}");
            }

            thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(windows)]
    async fn connect_test_named_pipe_client(pipe_name: &str) -> IpcClient {
        let mut client = IpcClient::new(IpcConfig {
            transport: IpcTransport::LocalSocket,
            endpoint: pipe_name.to_string(),
            connect_timeout_ms: 250,
            ..IpcConfig::default()
        });

        let start = tokio::time::Instant::now();
        loop {
            match client.connect().await {
                Ok(()) => return client,
                Err(err) if start.elapsed() < Duration::from_secs(3) => {
                    tracing::debug!(%err, pipe = %pipe_name, "Waiting for named-pipe IPC server");
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                Err(err) => panic!("named-pipe client failed to connect: {err}"),
            }
        }
    }

    fn spawn_mock_control_server(port: u16) -> thread::JoinHandle<()> {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .expect("mock control listener bind should succeed");
        thread::spawn(move || {
            let mut running = true;
            let mut calibration_mode = false;
            let mut output_enabled = true;

            while running {
                let (mut stream, _) = listener
                    .accept()
                    .expect("mock control listener accept should succeed");

                let request = read_control_request(&stream);
                let response = match request.command {
                    ControlCommand::Snapshot => ControlResponse::snapshot(
                        request.request_id,
                        ControlSnapshot {
                            running,
                            uptime_secs: 42,
                            calibration_mode,
                            output_enabled,
                            profile_ready: true,
                            decoder_ready: true,
                            decoder_model_version: Some("test-v1".to_string()),
                            active_profile_name: Some("test-profile".to_string()),
                            device_name: Some("Mock Device".to_string()),
                            device_battery: Some(88),
                            signal_quality: 0.9,
                            signal_latency_last_us: 100,
                            signal_latency_p95_us: 150,
                            decode_latency_last_us: 200,
                            decode_latency_p95_us: 240,
                            action_latency_last_us: 300,
                            action_latency_p95_us: 360,
                            latency_degraded: false,
                            latency_alert_message: None,
                            actions_emitted: 10,
                            errors_detected: 1,
                            ipc_connected: true,
                            ipc_simulated: false,
                            learning_enabled: true,
                            ml_bridge_connected: true,
                            ml_bridge_stalled: false,
                            runtime_mode_state: RuntimeModeState::Full,
                            enabled_capabilities: vec![
                                "cursor_move".to_string(),
                                "click".to_string(),
                                "keyboard".to_string(),
                            ],
                            limited_capabilities_message: None,
                            fallback_model_kind: Some("onnx".to_string()),
                            trainer_replay_size: Some(200),
                            trainer_step: Some(33),
                            trainer_policy_loss: Some(0.11),
                            trainer_value_loss: Some(0.22),
                            trainer_entropy: Some(0.03),
                            trainer_last_error: None,
                            candidate_promotions_succeeded: 2,
                            candidate_promotions_rejected: 1,
                            candidate_last_outcome: Some(
                                "candidate promotion accepted".to_string(),
                            ),
                            ml_protocol_version: Some(3),
                            device_connected: true,
                            task_error: None,
                            discovered_streams: vec![],
                            routed_eeg_streams: 1,
                            routed_motion_streams: 1,
                            routed_auxiliary_streams: 2,
                            routed_unknown_streams: 0,
                            pipeline_integrity_degraded: false,
                            integrity_issue_count: 0,
                            stage_health_summary: Some("signal:ok".to_string()),
                        },
                    ),
                    ControlCommand::SetCalibrationMode { enabled } => {
                        calibration_mode = enabled;
                        ControlResponse::ack(request.request_id)
                    }
                    ControlCommand::SetOutputEnabled { enabled } => {
                        output_enabled = enabled;
                        ControlResponse::ack(request.request_id)
                    }
                    ControlCommand::Shutdown => {
                        running = false;
                        ControlResponse::ack(request.request_id)
                    }
                    ControlCommand::TrainerSnapshot => ControlResponse::trainer_snapshot(
                        request.request_id,
                        neurohid_types::control::TrainerSnapshot {
                            trainer_connected: true,
                            trainer_state: "training".to_string(),
                            replay_size: 200,
                            training_step: 33,
                            last_heartbeat_us: Some(1),
                            last_error: None,
                            protocol_version: Some(3),
                        },
                    ),
                    ControlCommand::SetLearningEnabled { .. }
                    | ControlCommand::MlBridgeReconnect
                    | ControlCommand::SetFallbackPolicy { .. }
                    | ControlCommand::SetSignalConfig { .. } => {
                        ControlResponse::ack(request.request_id)
                    }
                    _ => ControlResponse {
                        request_id: request.request_id,
                        payload: ControlResponsePayload::Error {
                            message: "unsupported command in mock server".to_string(),
                        },
                    },
                };

                write_control_response(&mut stream, &response);
            }
        })
    }

    #[cfg(windows)]
    fn spawn_mock_named_pipe_control_server(pipe_name: String) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("named-pipe mock runtime should build");

            runtime.block_on(async move {
                let mut running = true;
                let mut calibration_mode = false;
                let mut output_enabled = true;

                while running {
                    let server = ServerOptions::new()
                        .create(&pipe_name)
                        .expect("named-pipe control server create should succeed");
                    server
                        .connect()
                        .await
                        .expect("named-pipe control server connect should succeed");

                    let (mut read_half, mut write_half) = tokio::io::split(server);

                    loop {
                        let envelope = match read_control_envelope_async(&mut read_half).await {
                            Some(envelope) => envelope,
                            None => break,
                        };
                        let parsed = if envelope.v == IPC_PROTOCOL_V3
                            && envelope.channel == IpcChannelV3::ControlRpc
                            && envelope.msg_type == "request"
                        {
                            envelope
                                .decode_payload::<ControlRpcRequestV3>()
                                .map(ControlRequest::from)
                        } else {
                            Err("invalid control envelope channel/msg_type".to_string())
                        };
                        let response = match parsed {
                            Ok(request) => match request.command {
                                ControlCommand::Snapshot => ControlResponse::snapshot(
                                    request.request_id,
                                    ControlSnapshot {
                                        running,
                                        uptime_secs: 42,
                                        calibration_mode,
                                        output_enabled,
                                        profile_ready: true,
                                        decoder_ready: true,
                                        decoder_model_version: Some("test-v1".to_string()),
                                        active_profile_name: Some("test-profile".to_string()),
                                        device_name: Some("Mock Device".to_string()),
                                        device_battery: Some(88),
                                        signal_quality: 0.9,
                                        signal_latency_last_us: 100,
                                        signal_latency_p95_us: 150,
                                        decode_latency_last_us: 200,
                                        decode_latency_p95_us: 240,
                                        action_latency_last_us: 300,
                                        action_latency_p95_us: 360,
                                        latency_degraded: false,
                                        latency_alert_message: None,
                                        actions_emitted: 10,
                                        errors_detected: 1,
                                        ipc_connected: true,
                                        ipc_simulated: false,
                                        learning_enabled: true,
                                        ml_bridge_connected: true,
                                        ml_bridge_stalled: false,
                                        runtime_mode_state: RuntimeModeState::Full,
                                        enabled_capabilities: vec![
                                            "cursor_move".to_string(),
                                            "click".to_string(),
                                            "keyboard".to_string(),
                                        ],
                                        limited_capabilities_message: None,
                                        fallback_model_kind: Some("onnx".to_string()),
                                        trainer_replay_size: Some(200),
                                        trainer_step: Some(33),
                                        trainer_policy_loss: Some(0.11),
                                        trainer_value_loss: Some(0.22),
                                        trainer_entropy: Some(0.03),
                                        trainer_last_error: None,
                                        candidate_promotions_succeeded: 2,
                                        candidate_promotions_rejected: 1,
                                        candidate_last_outcome: Some(
                                            "candidate promotion accepted".to_string(),
                                        ),
                                        ml_protocol_version: Some(3),
                                        device_connected: true,
                                        task_error: None,
                                        discovered_streams: vec![],
                                        routed_eeg_streams: 1,
                                        routed_motion_streams: 1,
                                        routed_auxiliary_streams: 2,
                                        routed_unknown_streams: 0,
                                        pipeline_integrity_degraded: false,
                                        integrity_issue_count: 0,
                                        stage_health_summary: Some("signal:ok".to_string()),
                                    },
                                ),
                                ControlCommand::SetCalibrationMode { enabled } => {
                                    calibration_mode = enabled;
                                    ControlResponse::ack(request.request_id)
                                }
                                ControlCommand::SetOutputEnabled { enabled } => {
                                    output_enabled = enabled;
                                    ControlResponse::ack(request.request_id)
                                }
                                ControlCommand::Shutdown => {
                                    running = false;
                                    ControlResponse::ack(request.request_id)
                                }
                                ControlCommand::SetLearningEnabled { .. }
                                | ControlCommand::MlBridgeReconnect
                                | ControlCommand::SetFallbackPolicy { .. }
                                | ControlCommand::SetSignalConfig { .. } => {
                                    ControlResponse::ack(request.request_id)
                                }
                                ControlCommand::TrainerSnapshot => {
                                    ControlResponse::trainer_snapshot(
                                        request.request_id,
                                        neurohid_types::control::TrainerSnapshot {
                                            trainer_connected: true,
                                            trainer_state: "training".to_string(),
                                            replay_size: 200,
                                            training_step: 33,
                                            last_heartbeat_us: Some(1),
                                            last_error: None,
                                            protocol_version: Some(3),
                                        },
                                    )
                                }
                                _ => ControlResponse {
                                    request_id: request.request_id,
                                    payload: ControlResponsePayload::Error {
                                        message: "unsupported command in named-pipe mock server"
                                            .to_string(),
                                    },
                                },
                            },
                            Err(error) => ControlResponse::error(
                                None,
                                format!("invalid control request payload: {}", error),
                            ),
                        };

                        write_control_response_async(&mut write_half, &response)
                            .await
                            .expect("named-pipe control response write should succeed");

                        if !running {
                            return;
                        }
                    }
                }
            });
        })
    }

    fn read_control_request(stream: &TcpStream) -> ControlRequest {
        let mut reader = stream
            .try_clone()
            .expect("mock stream clone should succeed");
        let envelope = read_control_envelope_sync(&mut reader)
            .expect("mock control request should be readable")
            .expect("mock control request should not be empty");
        assert_eq!(
            envelope.v, IPC_PROTOCOL_V3,
            "mock control request should use ipc v3"
        );
        assert_eq!(
            envelope.channel,
            IpcChannelV3::ControlRpc,
            "mock control request should target control.rpc channel"
        );
        assert_eq!(
            envelope.msg_type, "request",
            "mock control request should be request msg_type"
        );
        let request_v3 = envelope
            .decode_payload::<ControlRpcRequestV3>()
            .expect("mock control request payload should parse");
        ControlRequest::from(request_v3)
    }

    fn write_control_response(stream: &mut TcpStream, response: &ControlResponse) {
        let response_payload = ControlRpcResponseV3::from(response.clone());
        let envelope = IpcEnvelopeV3::new(
            IpcChannelV3::ControlRpc,
            "response",
            1,
            response.request_id.clone(),
            Some("mock-control".to_string()),
            &response_payload,
        )
        .expect("mock control response should encode");
        write_control_envelope_sync(stream, &envelope).expect("mock control response should write");
    }

    fn read_control_envelope_sync<R>(reader: &mut R) -> Result<Option<IpcEnvelopeV3>, String>
    where
        R: Read,
    {
        let mut len_buf = [0_u8; 4];
        match reader.read_exact(&mut len_buf) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(error) => return Err(format!("failed to read control frame length: {}", error)),
        }
        let frame_len = u32::from_le_bytes(len_buf) as usize;
        if frame_len == 0 {
            return Ok(None);
        }
        let mut payload = vec![0_u8; frame_len];
        reader
            .read_exact(&mut payload)
            .map_err(|e| format!("failed to read control frame payload: {}", e))?;
        let envelope = serde_json::from_slice::<IpcEnvelopeV3>(&payload)
            .map_err(|e| format!("failed to decode control envelope: {}", e))?;
        Ok(Some(envelope))
    }

    fn write_control_envelope_sync<W>(
        writer: &mut W,
        envelope: &IpcEnvelopeV3,
    ) -> Result<(), String>
    where
        W: Write,
    {
        let payload = serde_json::to_vec(envelope)
            .map_err(|e| format!("failed to encode control envelope: {}", e))?;
        let frame_len = payload.len() as u32;
        writer
            .write_all(&frame_len.to_le_bytes())
            .map_err(|e| format!("failed to write control frame length: {}", e))?;
        writer
            .write_all(&payload)
            .map_err(|e| format!("failed to write control frame payload: {}", e))?;
        writer
            .flush()
            .map_err(|e| format!("failed to flush control frame payload: {}", e))?;
        Ok(())
    }

    #[cfg(windows)]
    async fn read_control_envelope_async<R>(reader: &mut R) -> Option<IpcEnvelopeV3>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        let mut len_buf = [0_u8; 4];
        if reader.read_exact(&mut len_buf).await.is_err() {
            return None;
        }
        let frame_len = u32::from_le_bytes(len_buf) as usize;
        if frame_len == 0 {
            return None;
        }
        let mut payload = vec![0_u8; frame_len];
        if reader.read_exact(&mut payload).await.is_err() {
            return None;
        }
        serde_json::from_slice::<IpcEnvelopeV3>(&payload).ok()
    }

    #[cfg(windows)]
    async fn write_control_response_async<W>(
        writer: &mut W,
        response: &ControlResponse,
    ) -> Result<(), std::io::Error>
    where
        W: tokio::io::AsyncWrite + Unpin,
    {
        let response_payload = ControlRpcResponseV3::from(response.clone());
        let envelope = IpcEnvelopeV3::new(
            IpcChannelV3::ControlRpc,
            "response",
            1,
            response.request_id.clone(),
            Some("mock-control".to_string()),
            &response_payload,
        )
        .map_err(std::io::Error::other)?;
        let payload = serde_json::to_vec(&envelope).map_err(std::io::Error::other)?;
        let frame_len = payload.len() as u32;
        writer.write_all(&frame_len.to_le_bytes()).await?;
        writer.write_all(&payload).await?;
        writer.flush().await?;
        Ok(())
    }

    fn allocate_test_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("ephemeral port bind should succeed")
            .local_addr()
            .expect("local addr should resolve")
            .port()
    }

    #[cfg(windows)]
    fn unique_pipe_name(prefix: &str) -> String {
        format!(
            r"\\.\pipe\{}_{}_{}",
            prefix,
            std::process::id(),
            allocate_test_port()
        )
    }
}
