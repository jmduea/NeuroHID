//! # Service Manager
//!
//! Manages the lifecycle of the embedded NeuroHID service within the hub.
//! Provides start/stop, calibration mode toggle, and non-blocking state reads.

use tokio::sync::broadcast;

use neurohid_core::service::{DeviceCommand, NeuroHidService, ServiceHandle, SignalCommand};
use neurohid_storage::ProfileStore;
use neurohid_types::{
    config::{SignalConfig, SystemConfig},
    profile::ProfileId,
};

use crate::data_bus::DataBus;
use crate::state::ServiceSnapshot;

/// Manages the embedded service lifecycle.
pub struct ServiceManager {
    handle: Option<ServiceHandle>,
    last_error: Option<String>,
    /// Whether the data bus has been connected to this handle's broadcast receivers.
    bus_connected: bool,
    /// Cached snapshot from the last successful `try_read()`, returned when the
    /// lock is contended to avoid discarding `discovered_streams` for a frame.
    cached_snapshot: ServiceSnapshot,
}

impl ServiceManager {
    pub fn new() -> Self {
        Self {
            handle: None,
            last_error: None,
            bus_connected: false,
            cached_snapshot: ServiceSnapshot::default(),
        }
    }

    /// Start the service in the background.
    pub fn start(
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
        let should_drop_handle = self.handle.as_ref().is_some_and(|handle| {
            match handle.state.try_read() {
                Ok(state) => !state.active,
                // Lock contended — check if the join handle has already finished,
                // which is a reliable signal that the service is dead.
                Err(_) => handle.join_handle.is_finished(),
            }
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
                self.last_error = Some(format!("Failed to create service: {}", e));
                tracing::error!("Failed to start service: {}", e);
            }
        }
    }

    /// Stop the running service.
    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.shutdown_tx.send(());
            self.bus_connected = false;
            tracing::info!("Service stop signal sent");
        }
    }

    /// Synchronize the data bus with the service's broadcast channels.
    /// Connects on first call after start, disconnects when service stops.
    pub fn sync_data_bus(&mut self, bus: &mut DataBus) {
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

    /// Take a non-blocking snapshot of the service state.
    pub fn snapshot(&mut self) -> ServiceSnapshot {
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
            calibration_mode: state_guard.calibration_mode,
            output_enabled: state_guard.output_enabled,
            profile_ready: state_guard.profile_ready,
            decoder_ready: state_guard.decoder_ready,
            decoder_model_version: state_guard.decoder_model_version.clone(),
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

    /// Enter calibration mode (pauses HID emission, enables sample forwarding).
    pub fn enter_calibration_mode(&self) {
        if let Some(handle) = &self.handle {
            handle.set_calibration_mode(true);
            tracing::info!("Entered calibration mode");
        }
    }

    /// Exit calibration mode (resumes normal HID emission).
    pub fn exit_calibration_mode(&self) {
        if let Some(handle) = &self.handle {
            handle.set_calibration_mode(false);
            tracing::info!("Exited calibration mode");
        }
    }

    /// Enable or pause HID output.
    pub fn set_output_enabled(&self, enabled: bool) {
        if let Some(handle) = &self.handle {
            handle.set_output_enabled(enabled);
        }
    }

    /// Request decoder model reload for the active profile.
    pub fn reload_model(&self) {
        if let Some(handle) = &self.handle {
            handle.reload_model();
        }
    }

    /// Request guarded candidate model promotion for the active profile.
    pub fn promote_candidate_model(&self) {
        if let Some(handle) = &self.handle {
            handle.promote_candidate_model();
        }
    }

    /// Update active profile status used by runtime action gating.
    pub fn set_active_profile(
        &self,
        profile_id: Option<ProfileId>,
        profile_name: String,
        profile_ready: bool,
    ) {
        if let Some(handle) = &self.handle {
            handle.set_profile_status(profile_id, Some(profile_name), profile_ready);
        }
    }

    /// Whether the service is currently running.
    #[allow(dead_code)] // public API for future use
    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }

    /// Last error encountered during start.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Request a rescan of available LSL streams.
    pub fn rescan_streams(&self) {
        if let Some(handle) = &self.handle {
            let _ = handle.device_command_tx.try_send(DeviceCommand::Rescan);
        }
    }

    /// Connect to a specific stream by its id.
    pub fn connect_stream(&self, stream_id: &str) {
        if let Some(handle) = &self.handle {
            let _ = handle
                .device_command_tx
                .try_send(DeviceCommand::Connect(stream_id.to_string()));
        }
    }

    /// Disconnect from a specific stream by its id.
    pub fn disconnect_stream(&self, stream_id: &str) {
        if let Some(handle) = &self.handle {
            let _ = handle
                .device_command_tx
                .try_send(DeviceCommand::Disconnect(stream_id.to_string()));
        }
    }

    /// Connect to all streams in a group (e.g., all streams from one device).
    pub fn connect_streams(&self, stream_ids: &[&str]) {
        if let Some(handle) = &self.handle {
            for id in stream_ids {
                let _ = handle
                    .device_command_tx
                    .try_send(DeviceCommand::Connect(id.to_string()));
            }
        }
    }

    /// Disconnect from all streams in a group.
    pub fn disconnect_streams(&self, stream_ids: &[&str]) {
        if let Some(handle) = &self.handle {
            for id in stream_ids {
                let _ = handle
                    .device_command_tx
                    .try_send(DeviceCommand::Disconnect(id.to_string()));
            }
        }
    }

    /// Push a live signal configuration update into the running signal task.
    pub fn update_signal_config(&self, cfg: SignalConfig) {
        if let Some(handle) = &self.handle {
            let _ = handle
                .signal_command_tx
                .try_send(SignalCommand::UpdateConfig(cfg));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::thread;
    use std::time::{Duration, Instant};

    use neurohid_ipc::{IpcClient, IpcConfig, PythonToRust};
    use neurohid_types::config::{DeviceBackend, SystemConfig};

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
        config.service.ipc_port = allocate_test_port();

        manager.start(&runtime, config.clone(), None, None);
        wait_for_snapshot(&mut manager, Duration::from_secs(2), |snap| snap.running);

        let initial = manager.snapshot();
        assert!(!initial.ipc_connected);
        assert!(!initial.ipc_simulated);

        let mut client = runtime.block_on(connect_test_client(config.service.ipc_port));
        runtime.block_on(async {
            client
                .send(PythonToRust::Ready)
                .await
                .expect("ready should send");
        });

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

    fn allocate_test_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .expect("ephemeral port bind should succeed")
            .local_addr()
            .expect("local addr should resolve")
            .port()
    }
}
