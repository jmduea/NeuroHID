//! # Service Manager
//!
//! Manages the lifecycle of the embedded NeuroHID service within the hub.
//! Provides start/stop, calibration mode toggle, and non-blocking state reads.

use std::sync::atomic::Ordering;
use tokio::sync::broadcast;

use neurohid_types::{
    config::SystemConfig,
    profile::ProfileId,
};
use neurohid_storage::ProfileStore;
use neurohid_core::service::{NeuroHidService, ServiceHandle};

use crate::state::ServiceSnapshot;

/// Manages the embedded service lifecycle.
pub struct ServiceManager {
    handle: Option<ServiceHandle>,
    last_error: Option<String>,
}

impl ServiceManager {
    pub fn new() -> Self {
        Self {
            handle: None,
            last_error: None,
        }
    }

    /// Start the service in the background.
    pub fn start(
        &mut self,
        runtime: &tokio::runtime::Runtime,
        config: SystemConfig,
        profile_store: ProfileStore,
        profile_id: ProfileId,
    ) {
        // If we have a handle but the service has stopped itself (e.g., task failure),
        // drop the stale handle so the user can restart without clicking "Stop" first.
        let should_drop_handle = self.handle.as_ref().is_some_and(|handle| {
            handle.state.try_read().is_ok_and(|state| !state.active)
        });
        if should_drop_handle {
            self.handle.take();
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
            tracing::info!("Service stop signal sent");
        }
    }

    /// Take a non-blocking snapshot of the service state.
    pub fn snapshot(&self) -> ServiceSnapshot {
        let Some(handle) = &self.handle else {
            return ServiceSnapshot::default();
        };

        // try_read() is non-blocking — if the lock is held by a task,
        // we just return the previous snapshot values (stale by at most one frame).
        let state_guard = match handle.state.try_read() {
            Ok(guard) => guard,
            Err(_) => return ServiceSnapshot::default(),
        };

        let uptime_secs = state_guard
            .started_at
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);

        ServiceSnapshot {
            running: state_guard.active,
            device_connected: state_guard.device_connected,
            device_name: state_guard.device_name.clone(),
            device_battery: state_guard.device_battery,
            signal_quality: state_guard.signal_quality,
            actions_emitted: state_guard.actions_emitted,
            errors_detected: state_guard.errors_detected,
            uptime_secs,
            ipc_connected: state_guard.ipc_connected,
            calibration_mode: state_guard.calibration_mode,
            active_profile_name: state_guard.active_profile_name.clone(),
            task_error: state_guard.task_error.clone(),
        }
    }

    /// Enter calibration mode (pauses HID emission, enables sample forwarding).
    pub fn enter_calibration_mode(&self) {
        if let Some(handle) = &self.handle {
            handle.calibration_mode.store(true, Ordering::Relaxed);
            tracing::info!("Entered calibration mode");
        }
    }

    /// Exit calibration mode (resumes normal HID emission).
    pub fn exit_calibration_mode(&self) {
        if let Some(handle) = &self.handle {
            handle.calibration_mode.store(false, Ordering::Relaxed);
            tracing::info!("Exited calibration mode");
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
}
