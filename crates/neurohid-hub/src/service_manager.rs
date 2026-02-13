//! # Service Manager
//!
//! Manages runtime lifecycle and control for the hub.
//! Supports both embedded runtime hosting and optional external runtime control.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

use neurohid_core::service::{DeviceCommand, NeuroHidService, ServiceHandle, SignalCommand};
use neurohid_storage::ProfileStore;
use neurohid_types::{
    config::{ControlTransport, ServiceConfig, ServiceRuntimeMode, SignalConfig, SystemConfig},
    control::{
        ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload, ControlSnapshot,
    },
    profile::ProfileId,
};

use crate::data_bus::DataBus;
use crate::state::ServiceSnapshot;

const EXTERNAL_SNAPSHOT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const EXTERNAL_CONNECT_TIMEOUT: Duration = Duration::from_millis(120);
const EXTERNAL_IO_TIMEOUT: Duration = Duration::from_millis(250);

/// Manages service/runtime lifecycle for the hub.
pub struct ServiceManager {
    handle: Option<ServiceHandle>,
    last_error: Option<String>,
    /// Whether the data bus has been connected to this handle's broadcast receivers.
    bus_connected: bool,
    /// Cached snapshot from the last successful read.
    cached_snapshot: ServiceSnapshot,
    runtime_mode: ServiceRuntimeMode,
    external_control_transport: ControlTransport,
    external_control_endpoint: String,
    external_control_pipe_name: String,
    last_external_poll: Option<Instant>,
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
            external_control_transport: service_defaults.control_transport,
            external_control_endpoint: format!(
                "{}:{}",
                service_defaults.control_host, service_defaults.control_port
            ),
            external_control_pipe_name: service_defaults.control_pipe_name,
            last_external_poll: None,
        }
    }

    /// Synchronize manager mode/endpoint from latest config.
    pub fn configure(&mut self, config: &SystemConfig) {
        let next_mode = config.service.runtime_mode.clone();
        let host = if config.service.control_host.trim().is_empty() {
            "127.0.0.1"
        } else {
            config.service.control_host.trim()
        };
        let next_endpoint = format!("{}:{}", host, config.service.control_port);
        let next_transport = config.service.control_transport.clone();
        let next_pipe_name = config.service.control_pipe_name.clone();

        if self.runtime_mode != next_mode {
            if self.runtime_mode == ServiceRuntimeMode::Embedded {
                self.stop_embedded();
            } else {
                self.cached_snapshot = ServiceSnapshot::default();
            }
            self.last_external_poll = None;
        }

        if self.external_control_endpoint != next_endpoint
            || self.external_control_transport != next_transport
            || self.external_control_pipe_name != next_pipe_name
        {
            self.external_control_endpoint = next_endpoint;
            self.external_control_transport = next_transport;
            self.external_control_pipe_name = next_pipe_name;
            self.last_external_poll = None;
            if self.runtime_mode == ServiceRuntimeMode::External {
                self.cached_snapshot = ServiceSnapshot::default();
            }
        }

        self.runtime_mode = next_mode;
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
                self.start_embedded(runtime, config, profile_store, profile_id)
            }
            ServiceRuntimeMode::External => {
                self.stop_embedded();
                self.last_external_poll = None;
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
                let _ = cfg;
                tracing::debug!(
                    "Signal config hot-update is not available in external runtime mode"
                );
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
        };
        self.cached_snapshot = snap.clone();
        snap
    }

    fn snapshot_external(&mut self) -> ServiceSnapshot {
        let now = Instant::now();
        if let Some(last_poll) = self.last_external_poll {
            if now.duration_since(last_poll) < EXTERNAL_SNAPSHOT_POLL_INTERVAL {
                return self.cached_snapshot.clone();
            }
        }
        self.last_external_poll = Some(now);

        let endpoint = self.control_endpoint_label();
        match self.send_control_request(ControlRequest::new(ControlCommand::Snapshot)) {
            Ok(response) => match response.payload {
                ControlResponsePayload::Snapshot { snapshot } => {
                    self.cached_snapshot = Self::snapshot_from_control(snapshot);
                    self.last_error = None;
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
                }
                ControlResponsePayload::Ack => {
                    self.set_last_error(format!(
                        "External runtime at {} returned unexpected ACK for snapshot",
                        endpoint
                    ));
                    self.cached_snapshot = ServiceSnapshot::default();
                }
                ControlResponsePayload::TrainerSnapshot { .. } => {
                    self.set_last_error(format!(
                        "External runtime at {} returned unexpected trainer snapshot payload",
                        endpoint
                    ));
                    self.cached_snapshot = ServiceSnapshot::default();
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
        match self.external_control_transport {
            ControlTransport::NamedPipe => self.send_control_request_named_pipe(request),
            ControlTransport::TcpLoopback => self.send_control_request_tcp(request),
        }
    }

    fn send_control_request_tcp(&self, request: ControlRequest) -> Result<ControlResponse, String> {
        let mut addrs = self
            .external_control_endpoint
            .to_socket_addrs()
            .map_err(|e| {
                format!(
                    "invalid control endpoint '{}': {}",
                    self.external_control_endpoint, e
                )
            })?;

        let Some(addr) = addrs.next() else {
            return Err(format!(
                "control endpoint '{}' did not resolve to a socket address",
                self.external_control_endpoint
            ));
        };

        let mut stream = TcpStream::connect_timeout(&addr, EXTERNAL_CONNECT_TIMEOUT)
            .map_err(|e| format!("connect failed: {}", e))?;
        stream
            .set_read_timeout(Some(EXTERNAL_IO_TIMEOUT))
            .map_err(|e| format!("failed to set read timeout: {}", e))?;
        stream
            .set_write_timeout(Some(EXTERNAL_IO_TIMEOUT))
            .map_err(|e| format!("failed to set write timeout: {}", e))?;

        let payload = serde_json::to_string(&request)
            .map_err(|e| format!("failed to encode request payload: {}", e))?;
        stream
            .write_all(payload.as_bytes())
            .map_err(|e| format!("failed to write request payload: {}", e))?;
        stream
            .write_all(b"\n")
            .map_err(|e| format!("failed to terminate request line: {}", e))?;
        stream
            .flush()
            .map_err(|e| format!("failed to flush request payload: {}", e))?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|e| format!("failed to read response payload: {}", e))?;

        if line.trim().is_empty() {
            return Err("received empty response from control endpoint".to_string());
        }

        serde_json::from_str::<ControlResponse>(line.trim())
            .map_err(|e| format!("failed to decode control response: {}", e))
    }

    fn send_control_request_named_pipe(
        &self,
        request: ControlRequest,
    ) -> Result<ControlResponse, String> {
        #[cfg(windows)]
        {
            let mut stream = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.external_control_pipe_name)
                .map_err(|e| {
                    format!(
                        "failed to open control named pipe '{}': {}",
                        self.external_control_pipe_name, e
                    )
                })?;

            let payload = serde_json::to_string(&request)
                .map_err(|e| format!("failed to encode request payload: {}", e))?;
            stream
                .write_all(payload.as_bytes())
                .map_err(|e| format!("failed to write request payload: {}", e))?;
            stream
                .write_all(b"\n")
                .map_err(|e| format!("failed to terminate request line: {}", e))?;
            stream
                .flush()
                .map_err(|e| format!("failed to flush request payload: {}", e))?;

            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .map_err(|e| format!("failed to read response payload: {}", e))?;

            if line.trim().is_empty() {
                return Err("received empty response from control endpoint".to_string());
            }

            return serde_json::from_str::<ControlResponse>(line.trim())
                .map_err(|e| format!("failed to decode control response: {}", e));
        }
        #[cfg(not(windows))]
        {
            let _ = request;
            Err("named-pipe control transport is only supported on Windows".to_string())
        }
    }

    fn control_endpoint_label(&self) -> String {
        match self.external_control_transport {
            ControlTransport::NamedPipe => self.external_control_pipe_name.clone(),
            ControlTransport::TcpLoopback => self.external_control_endpoint.clone(),
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
            ..ServiceSnapshot::default()
        }
    }

    fn set_last_error(&mut self, message: String) {
        if self.last_error.as_deref() != Some(message.as_str()) {
            tracing::error!("{}", message);
        }
        self.last_error = Some(message);
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::{Duration, Instant};

    use neurohid_ipc::{IpcClient, IpcConfig, IpcTransport};
    use neurohid_types::{
        config::{ControlTransport, DeviceBackend, MlTransport, ServiceRuntimeMode, SystemConfig},
        control::{
            ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload,
            ControlSnapshot, RuntimeModeState,
        },
    };

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
        config.service.ml_transport = MlTransport::TcpLoopback;
        config.service.ipc_port = allocate_test_port();

        manager.start(&runtime, config.clone(), None, None);
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| snap.running);

        let initial = manager.snapshot();
        assert!(!initial.ipc_connected);
        assert!(!initial.ipc_simulated);

        let mut client = runtime.block_on(connect_test_client(config.service.ipc_port));

        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| {
            snap.ipc_connected && !snap.ipc_simulated
        });

        runtime.block_on(async {
            client
                .disconnect()
                .await
                .expect("disconnect should succeed");
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

        manager.stop();
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| !snap.running);

        server_join
            .join()
            .expect("mock control server thread should join");
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

    async fn connect_test_client(port: u16) -> IpcClient {
        let mut client = IpcClient::new(IpcConfig {
            transport: IpcTransport::TcpLoopback,
            address: format!("127.0.0.1:{port}"),
            connect_timeout_ms: 250,
            ..IpcConfig::default()
        });

        let start = tokio::time::Instant::now();
        loop {
            match client.connect().await {
                Ok(()) => return client,
                Err(err) if start.elapsed() < Duration::from_secs(2) => {
                    tracing::debug!(%err, "Waiting for service IPC server");
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                Err(err) => panic!("client failed to connect: {err}"),
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
                            ml_protocol_version: Some(2),
                            device_connected: true,
                            task_error: None,
                            discovered_streams: vec![],
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
                            protocol_version: Some(2),
                        },
                    ),
                    ControlCommand::SetLearningEnabled { .. }
                    | ControlCommand::MlBridgeReconnect
                    | ControlCommand::SetFallbackPolicy { .. } => {
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

    fn read_control_request(stream: &TcpStream) -> ControlRequest {
        let mut line = String::new();
        let mut reader = BufReader::new(
            stream
                .try_clone()
                .expect("mock stream clone should succeed"),
        );
        reader
            .read_line(&mut line)
            .expect("mock control request should be readable");
        serde_json::from_str(line.trim()).expect("mock control request should parse")
    }

    fn write_control_response(stream: &mut TcpStream, response: &ControlResponse) {
        let payload = serde_json::to_string(response).expect("mock control response should encode");
        stream
            .write_all(payload.as_bytes())
            .expect("mock control response write should succeed");
        stream
            .write_all(b"\n")
            .expect("mock control response newline should write");
        stream
            .flush()
            .expect("mock control response flush should succeed");
    }

    fn allocate_test_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("ephemeral port bind should succeed")
            .local_addr()
            .expect("local addr should resolve")
            .port()
    }
}
