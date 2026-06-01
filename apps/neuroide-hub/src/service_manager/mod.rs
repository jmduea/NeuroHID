//! # Service Manager
//!
//! Manages runtime lifecycle and control for the hub.
//! Supports both embedded runtime hosting and optional external runtime control.

mod commands;
mod embedded;
mod external;
mod snapshot;
#[cfg(test)]
mod tests;

use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use tokio::sync::watch;

use neurohid_core::observability::EmitGate;
use neurohid_core::runtime::{RuntimeCommand, RuntimeHandle};
use neurohid_storage::ProfileStore;
use neurohid_types::observability::ObservabilityComponent;
use neurohid_types::{
    config::{
        FallbackPolicy, IpcMode, ServiceConfig, ServiceRuntimeMode, SignalConfig, SystemConfig,
    },
    control::{ControlCommand, ControlRequest, ControlResponsePayload},
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
    runtime_handle: Option<RuntimeHandle>,
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
            runtime_handle: None,
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
        let next_mode = config.service.runtime_mode.clone();
        let next_ipc_mode = config.service.ipc_mode.clone();
        let next_ipc_endpoint = config.service.ipc_endpoint.clone();

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
                            self.cached_snapshot = snapshot;
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
                        ControlResponsePayload::TrainerSnapshot { .. }
                        | ControlResponsePayload::RecordingStarted { .. }
                        | ControlResponsePayload::RecordingStopped { .. } => {
                            self.set_last_error(format!(
                                "External runtime at {} returned unexpected payload for snapshot request",
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
                        ControlResponsePayload::TrainerSnapshot { .. }
                        | ControlResponsePayload::RecordingStarted { .. }
                        | ControlResponsePayload::RecordingStopped { .. } => {
                            self.set_last_error(
                                "External runtime returned unexpected payload for shutdown"
                                    .to_string(),
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

        if let Some(rt) = &self.runtime_handle {
            let active = rt.is_alive();

            if active && !self.bus_connected {
                bus.connect(
                    rt.subscribe_samples(),
                    rt.subscribe_features(),
                    rt.subscribe_actions(),
                    rt.subscribe_markers(),
                );
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::ToggleCalibration { enabled: true });
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::ToggleCalibration { enabled: false });
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::ToggleOutput { enabled });
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::ReloadModel);
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::PromoteCandidateModel);
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::SetLearningEnabled { enabled });
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::MlBridgeReconnect);
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::SetFallbackPolicy { policy });
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::SetFallbackPolicy { policy });
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::SetProfileStatus {
                        profile_id,
                        profile_name: Some(profile_name),
                        profile_ready,
                    });
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
    pub fn is_running(&self) -> bool {
        match self.runtime_mode {
            ServiceRuntimeMode::Embedded => self.runtime_handle.is_some(),
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::RescanStreams);
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::ConnectStream {
                        stream_id: stream_id.to_string(),
                    });
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::DisconnectStream {
                        stream_id: stream_id.to_string(),
                    });
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
                if let Some(rt) = &self.runtime_handle {
                    let _ = rt.command(RuntimeCommand::SetSignalConfig { signal: cfg });
                }
            }
            ServiceRuntimeMode::External => {
                self.send_external_command(ControlCommand::SetSignalConfig { signal: cfg });
            }
        }
    }
}
