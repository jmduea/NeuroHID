//! # Managed Runtime API
//!
//! Stable facade for embedding the NeuroHID runtime in first-party or
//! third-party applications.

use tokio::sync::broadcast;

use neurohid_storage::ProfileStore;
use neurohid_types::{
    config::SystemConfig,
    error::{Error, Result},
    profile::ProfileId,
};

use crate::service::{DeviceCommand, NeuroHidService, ServiceHandle};

/// Builder for a managed runtime instance.
pub struct RuntimeBuilder {
    config: SystemConfig,
    profile_store: Option<ProfileStore>,
    profile_id: Option<ProfileId>,
}

impl RuntimeBuilder {
    /// Create a builder from runtime configuration.
    pub fn new(config: SystemConfig) -> Self {
        Self {
            config,
            profile_store: None,
            profile_id: None,
        }
    }

    /// Attach an initialized profile store.
    pub fn with_profile_store(mut self, store: ProfileStore) -> Self {
        self.profile_store = Some(store);
        self
    }

    /// Select the active profile.
    pub fn with_profile_id(mut self, profile_id: ProfileId) -> Self {
        self.profile_id = Some(profile_id);
        self
    }

    /// Start runtime tasks and return a managed handle.
    pub async fn start(self) -> Result<RuntimeHandle> {
        let (shutdown_tx, shutdown_rx) = broadcast::channel::<()>(1);
        let service = NeuroHidService::new(
            self.config,
            self.profile_store,
            self.profile_id,
            shutdown_rx,
        )
        .await?;

        let handle = service.spawn(shutdown_tx);
        Ok(RuntimeHandle { handle })
    }
}

/// Commands supported by the managed runtime.
#[derive(Debug, Clone)]
pub enum RuntimeCommand {
    /// Start command (no-op once runtime is running).
    Start,
    /// Stop all runtime tasks.
    Stop,
    /// Re-scan available acquisition streams.
    RescanStreams,
    /// Connect a specific discovered stream.
    ConnectStream { stream_id: String },
    /// Disconnect a specific discovered stream.
    DisconnectStream { stream_id: String },
    /// Toggle calibration mode.
    ToggleCalibration { enabled: bool },
    /// Pause or resume HID output.
    ToggleOutput { enabled: bool },
    /// Reload active inference model.
    ReloadModel,
    /// Promote a validated candidate model and hot-swap runtime inference.
    PromoteCandidateModel,
}

/// Snapshot of runtime state for host applications.
#[derive(Debug, Clone)]
pub struct RuntimeSnapshot {
    pub running: bool,
    pub calibration_mode: bool,
    pub output_enabled: bool,
    pub profile_ready: bool,
    pub decoder_ready: bool,
    pub decoder_model_version: Option<String>,
    pub active_profile_name: Option<String>,
    pub signal_quality: f32,
    pub decode_latency_last_us: u64,
    pub decode_latency_p95_us: u64,
    pub signal_latency_last_us: u64,
    pub signal_latency_p95_us: u64,
    pub action_latency_last_us: u64,
    pub action_latency_p95_us: u64,
    pub latency_degraded: bool,
    pub latency_alert_message: Option<String>,
    pub actions_emitted: u64,
    pub errors_detected: u64,
    pub device_connected: bool,
    pub ipc_connected: bool,
}

impl Default for RuntimeSnapshot {
    fn default() -> Self {
        Self {
            running: false,
            calibration_mode: false,
            output_enabled: true,
            profile_ready: false,
            decoder_ready: false,
            decoder_model_version: None,
            active_profile_name: None,
            signal_quality: 0.0,
            decode_latency_last_us: 0,
            decode_latency_p95_us: 0,
            signal_latency_last_us: 0,
            signal_latency_p95_us: 0,
            action_latency_last_us: 0,
            action_latency_p95_us: 0,
            latency_degraded: false,
            latency_alert_message: None,
            actions_emitted: 0,
            errors_detected: 0,
            device_connected: false,
            ipc_connected: false,
        }
    }
}

/// Handle to a running managed runtime.
pub struct RuntimeHandle {
    handle: ServiceHandle,
}

impl RuntimeHandle {
    /// Send a command to the runtime.
    pub fn command(&self, command: RuntimeCommand) -> Result<()> {
        match command {
            RuntimeCommand::Start => Ok(()),
            RuntimeCommand::Stop => {
                let _ = self.handle.shutdown_tx.send(());
                Ok(())
            }
            RuntimeCommand::RescanStreams => {
                self.handle
                    .device_command_tx
                    .try_send(DeviceCommand::Rescan)
                    .map_err(|e| Error::Internal(format!("failed to send rescan command: {e}")))?;
                Ok(())
            }
            RuntimeCommand::ConnectStream { stream_id } => {
                self.handle
                    .device_command_tx
                    .try_send(DeviceCommand::Connect(stream_id))
                    .map_err(|e| Error::Internal(format!("failed to send connect command: {e}")))?;
                Ok(())
            }
            RuntimeCommand::DisconnectStream { stream_id } => {
                self.handle
                    .device_command_tx
                    .try_send(DeviceCommand::Disconnect(stream_id))
                    .map_err(|e| {
                        Error::Internal(format!("failed to send disconnect command: {e}"))
                    })?;
                Ok(())
            }
            RuntimeCommand::ToggleCalibration { enabled } => {
                self.handle.set_calibration_mode(enabled);
                Ok(())
            }
            RuntimeCommand::ToggleOutput { enabled } => {
                self.handle.set_output_enabled(enabled);
                Ok(())
            }
            RuntimeCommand::ReloadModel => {
                self.handle.reload_model();
                Ok(())
            }
            RuntimeCommand::PromoteCandidateModel => {
                self.handle.promote_candidate_model();
                Ok(())
            }
        }
    }

    /// Read a non-blocking runtime snapshot.
    pub fn snapshot(&self) -> RuntimeSnapshot {
        let Ok(state) = self.handle.state.try_read() else {
            return RuntimeSnapshot::default();
        };

        RuntimeSnapshot {
            running: state.active,
            calibration_mode: state.calibration_mode,
            output_enabled: state.output_enabled,
            profile_ready: state.profile_ready,
            decoder_ready: state.decoder_ready,
            decoder_model_version: state.decoder_model_version.clone(),
            active_profile_name: state.active_profile_name.clone(),
            signal_quality: state.signal_quality,
            decode_latency_last_us: state.decode_latency_last_us,
            decode_latency_p95_us: state.decode_latency_p95_us,
            signal_latency_last_us: state.signal_latency_last_us,
            signal_latency_p95_us: state.signal_latency_p95_us,
            action_latency_last_us: state.action_latency_last_us,
            action_latency_p95_us: state.action_latency_p95_us,
            latency_degraded: state.latency_degraded,
            latency_alert_message: state.latency_alert_message.clone(),
            actions_emitted: state.actions_emitted,
            errors_detected: state.errors_detected,
            device_connected: state.device_connected,
            ipc_connected: state.ipc_connected,
        }
    }

    /// Wait for runtime termination.
    pub async fn wait(self) -> Result<()> {
        self.handle
            .join_handle
            .await
            .map_err(|e| Error::Internal(format!("runtime join failed: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use neurohid_types::config::{DeviceBackend, SystemConfig};

    use super::{RuntimeBuilder, RuntimeCommand};

    async fn wait_for<F>(timeout: Duration, mut predicate: F)
    where
        F: FnMut() -> bool,
    {
        let deadline = tokio::time::Instant::now() + timeout;
        while tokio::time::Instant::now() < deadline {
            if predicate() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("timed out waiting for runtime condition");
    }

    #[tokio::test]
    async fn managed_runtime_handles_control_commands_and_shutdown() {
        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::Mock;
        config.service.ipc_simulation_enabled = true;
        config.action.enabled = false;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");

        wait_for(Duration::from_secs(3), || runtime.snapshot().running).await;

        runtime
            .command(RuntimeCommand::ToggleCalibration { enabled: true })
            .expect("toggle calibration should succeed");
        wait_for(Duration::from_secs(1), || {
            runtime.snapshot().calibration_mode
        })
        .await;

        runtime
            .command(RuntimeCommand::ToggleOutput { enabled: false })
            .expect("toggle output should succeed");
        wait_for(Duration::from_secs(1), || {
            !runtime.snapshot().output_enabled
        })
        .await;

        runtime
            .command(RuntimeCommand::RescanStreams)
            .expect("rescan should succeed");
        runtime
            .command(RuntimeCommand::ReloadModel)
            .expect("reload model should succeed");
        runtime
            .command(RuntimeCommand::PromoteCandidateModel)
            .expect("promote candidate should succeed");

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }
}
