//! # Managed Runtime API
//!
//! Stable facade for embedding the NeuroHID runtime in first-party or
//! third-party applications.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::{broadcast, mpsc};

use neurohid_storage::ProfileStore;
use neurohid_types::{
    IpcEnvelope,
    action::Action,
    config::{FallbackPolicy, SignalConfig, SystemConfig},
    control::{ControlCommand, ControlRequest, ControlResponse, ControlSnapshot, TrainerSnapshot},
    error::{Error, Result},
    event::StreamMarker,
    ipc::RuntimeEvent,
    profile::ProfileId,
    signal::{FeatureVector, Sample},
};

use crate::service::{DeviceCommand, NeuroHidService, ServiceHandle, ServiceState, SignalCommand};
use crate::tasks::TrainerIngressEvent;

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
    /// Replace signal processing configuration at runtime.
    SetSignalConfig { signal: SignalConfig },
    /// Enable or disable runtime learning.
    SetLearningEnabled { enabled: bool },
    /// Trigger an ML bridge reconnect attempt.
    MlBridgeReconnect,
    /// Push fallback policy settings into the running runtime.
    SetFallbackPolicy { policy: FallbackPolicy },
    /// Update active profile status used by runtime action gating.
    SetProfileStatus {
        profile_id: Option<ProfileId>,
        profile_name: Option<String>,
        profile_ready: bool,
    },
}

/// Type alias for the canonical runtime snapshot.
///
/// Previously a standalone struct with fields duplicated from
/// [`ControlSnapshot`]. Now unified to eliminate field-drift risk.
pub type RuntimeSnapshot = ControlSnapshot;

/// Handle to a running managed runtime.
pub struct RuntimeHandle {
    handle: ServiceHandle,
}

impl RuntimeHandle {
    /// Check whether the runtime is still alive.
    ///
    /// Returns `true` if the runtime task is active or the state lock is
    /// contended (assumed alive). Returns `false` only when the state is
    /// confirmed inactive or the join handle has finished.
    pub fn is_alive(&self) -> bool {
        match self.handle.state.try_read() {
            Ok(state) => state.active,
            Err(_) => !self.handle.join_handle.is_finished(),
        }
    }
}

/// Cloneable runtime facade used by IPC/control servers.
#[derive(Clone)]
pub struct RuntimeIpcHandle {
    state: Arc<tokio::sync::RwLock<ServiceState>>,
    shutdown_tx: broadcast::Sender<()>,
    device_command_tx: mpsc::Sender<DeviceCommand>,
    signal_command_tx: mpsc::Sender<SignalCommand>,
    decoder_command_tx: mpsc::Sender<crate::service::DecoderCommand>,
    calibration_mode: Arc<AtomicBool>,
    output_enabled: Arc<AtomicBool>,
    sample_broadcast_tx: broadcast::Sender<Sample>,
    feature_broadcast_tx: broadcast::Sender<FeatureVector>,
    action_broadcast_tx: broadcast::Sender<Action>,
    marker_broadcast_tx: broadcast::Sender<StreamMarker>,
    runtime_event_broadcast_tx: broadcast::Sender<RuntimeEvent>,
    trainer_ingress_tx: mpsc::Sender<TrainerIngressEvent>,
    trainer_egress_rx: Arc<tokio::sync::Mutex<mpsc::Receiver<IpcEnvelope>>>,
}

impl RuntimeHandle {
    /// Build a cloneable runtime facade suitable for concurrent IPC tasks.
    pub fn ipc_handle(&self) -> RuntimeIpcHandle {
        RuntimeIpcHandle {
            state: Arc::clone(&self.handle.state),
            shutdown_tx: self.handle.shutdown_tx.clone(),
            device_command_tx: self.handle.device_command_tx.clone(),
            signal_command_tx: self.handle.signal_command_tx.clone(),
            decoder_command_tx: self.handle.decoder_command_tx.clone(),
            calibration_mode: Arc::clone(&self.handle.calibration_mode),
            output_enabled: Arc::clone(&self.handle.output_enabled),
            sample_broadcast_tx: self.handle.sample_broadcast_tx.clone(),
            feature_broadcast_tx: self.handle.feature_broadcast_tx.clone(),
            action_broadcast_tx: self.handle.action_broadcast_tx.clone(),
            marker_broadcast_tx: self.handle.marker_broadcast_tx.clone(),
            runtime_event_broadcast_tx: self.handle.runtime_event_broadcast_tx.clone(),
            trainer_ingress_tx: self.handle.trainer_ingress_tx.clone(),
            trainer_egress_rx: Arc::clone(&self.handle.trainer_egress_rx),
        }
    }

    /// Subscribe to live runtime sample stream.
    pub fn subscribe_samples(&self) -> broadcast::Receiver<Sample> {
        self.ipc_handle().subscribe_samples()
    }

    /// Subscribe to live runtime feature stream.
    pub fn subscribe_features(&self) -> broadcast::Receiver<FeatureVector> {
        self.ipc_handle().subscribe_features()
    }

    /// Subscribe to live runtime action stream.
    pub fn subscribe_actions(&self) -> broadcast::Receiver<Action> {
        self.ipc_handle().subscribe_actions()
    }

    /// Subscribe to runtime marker/event annotations.
    pub fn subscribe_markers(&self) -> broadcast::Receiver<StreamMarker> {
        self.ipc_handle().subscribe_markers()
    }

    /// Send a command to the runtime.
    pub fn command(&self, command: RuntimeCommand) -> Result<()> {
        self.ipc_handle().command(command)
    }

    /// Read a non-blocking runtime snapshot.
    pub fn snapshot(&self) -> RuntimeSnapshot {
        self.ipc_handle().snapshot()
    }

    /// Build trainer bridge snapshot from current runtime state.
    pub fn trainer_snapshot(&self) -> TrainerSnapshot {
        self.ipc_handle().trainer_snapshot()
    }

    /// Handle one serialized control request and emit a serialized response.
    pub fn dispatch_control_request(&self, request: ControlRequest) -> ControlResponse {
        self.ipc_handle().dispatch_control_request(request)
    }

    /// Wait for runtime termination.
    pub async fn wait(self) -> Result<()> {
        self.handle
            .join_handle
            .await
            .map_err(|e| Error::Internal(format!("runtime join failed: {e}")))?
    }
}

impl RuntimeIpcHandle {
    /// Subscribe to live runtime sample stream.
    pub fn subscribe_samples(&self) -> broadcast::Receiver<Sample> {
        self.sample_broadcast_tx.subscribe()
    }

    /// Subscribe to live runtime feature stream.
    pub fn subscribe_features(&self) -> broadcast::Receiver<FeatureVector> {
        self.feature_broadcast_tx.subscribe()
    }

    /// Subscribe to live runtime action stream.
    pub fn subscribe_actions(&self) -> broadcast::Receiver<Action> {
        self.action_broadcast_tx.subscribe()
    }

    /// Subscribe to runtime marker/event annotations.
    pub fn subscribe_markers(&self) -> broadcast::Receiver<StreamMarker> {
        self.marker_broadcast_tx.subscribe()
    }

    /// Subscribe to runtime bridge-derived events (decision/ErrP/integrity stream).
    pub fn subscribe_runtime_bridge_events(&self) -> broadcast::Receiver<RuntimeEvent> {
        self.runtime_event_broadcast_tx.subscribe()
    }

    /// Notify trainer transport connection with the resolved session id.
    pub async fn trainer_connected(&self, session_id: String) -> Result<()> {
        self.trainer_ingress_tx
            .send(TrainerIngressEvent::Connected { session_id })
            .await
            .map_err(|error| {
                Error::Internal(format!(
                    "failed to forward trainer connected event: {error}"
                ))
            })
    }

    /// Forward one trainer-stream envelope into the runtime bridge engine.
    pub async fn trainer_send_envelope(&self, envelope: IpcEnvelope) -> Result<()> {
        self.trainer_ingress_tx
            .send(TrainerIngressEvent::Envelope(envelope))
            .await
            .map_err(|error| {
                Error::Internal(format!(
                    "failed to forward trainer envelope to runtime: {error}"
                ))
            })
    }

    /// Notify trainer transport disconnection.
    pub async fn trainer_disconnected(&self) -> Result<()> {
        self.trainer_ingress_tx
            .send(TrainerIngressEvent::Disconnected)
            .await
            .map_err(|error| {
                Error::Internal(format!(
                    "failed to forward trainer disconnected event: {error}"
                ))
            })
    }

    /// Receive one trainer-stream envelope produced by the runtime bridge engine.
    pub async fn recv_trainer_envelope(&self) -> Option<IpcEnvelope> {
        let mut rx = self.trainer_egress_rx.lock().await;
        rx.recv().await
    }

    /// Send a command to the runtime.
    pub fn command(&self, command: RuntimeCommand) -> Result<()> {
        match command {
            RuntimeCommand::Start => Ok(()),
            RuntimeCommand::Stop => {
                let _ = self.shutdown_tx.send(());
                Ok(())
            }
            RuntimeCommand::RescanStreams => {
                self.device_command_tx
                    .try_send(DeviceCommand::Rescan)
                    .map_err(|e| Error::Internal(format!("failed to send rescan command: {e}")))?;
                Ok(())
            }
            RuntimeCommand::ConnectStream { stream_id } => {
                self.device_command_tx
                    .try_send(DeviceCommand::Connect(stream_id))
                    .map_err(|e| Error::Internal(format!("failed to send connect command: {e}")))?;
                Ok(())
            }
            RuntimeCommand::DisconnectStream { stream_id } => {
                self.device_command_tx
                    .try_send(DeviceCommand::Disconnect(stream_id))
                    .map_err(|e| {
                        Error::Internal(format!("failed to send disconnect command: {e}"))
                    })?;
                Ok(())
            }
            RuntimeCommand::ToggleCalibration { enabled } => {
                self.calibration_mode.store(enabled, Ordering::Relaxed);
                if let Ok(mut state) = self.state.try_write() {
                    state.calibration_mode = enabled;
                } else if tokio::runtime::Handle::try_current().is_err() {
                    let mut state = self.state.blocking_write();
                    state.calibration_mode = enabled;
                }
                Ok(())
            }
            RuntimeCommand::ToggleOutput { enabled } => {
                self.output_enabled.store(enabled, Ordering::Relaxed);
                if let Ok(mut state) = self.state.try_write() {
                    state.output_enabled = enabled;
                } else if tokio::runtime::Handle::try_current().is_err() {
                    let mut state = self.state.blocking_write();
                    state.output_enabled = enabled;
                }
                Ok(())
            }
            RuntimeCommand::ReloadModel => {
                let _ = self
                    .decoder_command_tx
                    .try_send(crate::service::DecoderCommand::ReloadModel);
                Ok(())
            }
            RuntimeCommand::PromoteCandidateModel => {
                let _ = self
                    .decoder_command_tx
                    .try_send(crate::service::DecoderCommand::PromoteCandidateModel);
                Ok(())
            }
            RuntimeCommand::SetSignalConfig { signal } => {
                self.signal_command_tx
                    .try_send(SignalCommand::UpdateConfig(signal))
                    .map_err(|e| {
                        Error::Internal(format!("failed to send signal config command: {e}"))
                    })?;
                Ok(())
            }
            RuntimeCommand::SetLearningEnabled { enabled } => {
                if let Ok(mut state) = self.state.try_write() {
                    state.learning_enabled = enabled;
                } else if tokio::runtime::Handle::try_current().is_err() {
                    let mut state = self.state.blocking_write();
                    state.learning_enabled = enabled;
                }
                Ok(())
            }
            RuntimeCommand::MlBridgeReconnect => {
                if let Ok(mut state) = self.state.try_write() {
                    state.ml_bridge_stalled = false;
                } else if tokio::runtime::Handle::try_current().is_err() {
                    let mut state = self.state.blocking_write();
                    state.ml_bridge_stalled = false;
                }
                Ok(())
            }
            RuntimeCommand::SetFallbackPolicy { policy } => {
                if let Ok(mut state) = self.state.try_write() {
                    state.fallback_policy = policy;
                } else if tokio::runtime::Handle::try_current().is_err() {
                    let mut state = self.state.blocking_write();
                    state.fallback_policy = policy;
                }
                Ok(())
            }
            RuntimeCommand::SetProfileStatus {
                profile_id,
                profile_name,
                profile_ready,
            } => {
                let _ = self
                    .decoder_command_tx
                    .try_send(crate::service::DecoderCommand::SetActiveProfile { profile_id });
                if let Ok(mut state) = self.state.try_write() {
                    state.active_profile_name = profile_name;
                    state.profile_ready = profile_ready;
                } else if tokio::runtime::Handle::try_current().is_err() {
                    let mut state = self.state.blocking_write();
                    state.active_profile_name = profile_name;
                    state.profile_ready = profile_ready;
                }
                Ok(())
            }
        }
    }

    /// Read a non-blocking runtime snapshot.
    pub fn snapshot(&self) -> RuntimeSnapshot {
        let Ok(state) = self.state.try_read() else {
            return ControlSnapshot::default();
        };
        let uptime_secs = state.started_at.map(|t| t.elapsed().as_secs()).unwrap_or(0);

        ControlSnapshot {
            running: state.active,
            uptime_secs,
            calibration_mode: state.calibration_mode,
            output_enabled: state.output_enabled,
            profile_ready: state.profile_ready,
            decoder_ready: state.decoder_ready,
            decoder_model_version: state.decoder_model_version.clone(),
            active_profile_name: state.active_profile_name.clone(),
            device_name: state.device_name.clone(),
            device_battery: state.device_battery,
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
            ipc_simulated: state.ipc_simulated,
            task_error: state.task_error.clone(),
            discovered_streams: state.discovered_streams.clone(),
            routed_eeg_streams: state.routed_eeg_streams,
            routed_motion_streams: state.routed_motion_streams,
            routed_auxiliary_streams: state.routed_auxiliary_streams,
            routed_unknown_streams: state.routed_unknown_streams,
            pipeline_integrity_degraded: state.pipeline_integrity_degraded,
            integrity_issue_count: state.integrity_issue_count,
            stage_health_summary: state.stage_health_summary.clone(),
            learning_enabled: state.learning_enabled,
            ml_bridge_connected: state.ml_bridge_connected,
            ml_bridge_stalled: state.ml_bridge_stalled,
            runtime_mode_state: state.runtime_mode_state,
            enabled_capabilities: state.enabled_capabilities.clone(),
            limited_capabilities_message: state.limited_capabilities_message.clone(),
            fallback_model_kind: state.fallback_model_kind.clone(),
            trainer_replay_size: state.trainer_replay_size,
            trainer_step: state.trainer_step,
            trainer_policy_loss: state.trainer_policy_loss,
            trainer_value_loss: state.trainer_value_loss,
            trainer_entropy: state.trainer_entropy,
            trainer_last_error: state.trainer_last_error.clone(),
            candidate_promotions_succeeded: state.candidate_promotions_succeeded,
            candidate_promotions_rejected: state.candidate_promotions_rejected,
            candidate_last_outcome: state.candidate_last_outcome.clone(),
            ml_protocol_version: state.ml_protocol_version,
        }
    }

    /// Build trainer bridge snapshot from current runtime state.
    pub fn trainer_snapshot(&self) -> TrainerSnapshot {
        let snap = self.snapshot();
        TrainerSnapshot {
            trainer_connected: snap.ml_bridge_connected,
            trainer_state: if snap.ml_bridge_stalled {
                "stalled".to_string()
            } else if snap.ml_bridge_connected {
                "connected".to_string()
            } else {
                "disconnected".to_string()
            },
            replay_size: snap.trainer_replay_size.unwrap_or(0),
            training_step: snap.trainer_step.unwrap_or(0),
            last_heartbeat_us: self.last_ml_heartbeat_us(),
            last_error: snap
                .trainer_last_error
                .clone()
                .or_else(|| snap.task_error.map(|(_, e)| e)),
            protocol_version: snap.ml_protocol_version,
        }
    }

    fn last_ml_heartbeat_us(&self) -> Option<i64> {
        if let Ok(state) = self.state.try_read() {
            return state.ml_bridge_last_heartbeat_us;
        }
        None
    }

    /// Handle one serialized control request and emit a serialized response.
    pub fn dispatch_control_request(&self, request: ControlRequest) -> ControlResponse {
        let request_id = request.request_id.clone();
        match request.command {
            ControlCommand::Snapshot => ControlResponse::snapshot(request_id, self.snapshot()),
            ControlCommand::Shutdown => match self.command(RuntimeCommand::Stop) {
                Ok(()) => ControlResponse::ack(request_id),
                Err(error) => ControlResponse::error(request_id, error.to_string()),
            },
            ControlCommand::SetCalibrationMode { enabled } => {
                match self.command(RuntimeCommand::ToggleCalibration { enabled }) {
                    Ok(()) => ControlResponse::ack(request_id),
                    Err(error) => ControlResponse::error(request_id, error.to_string()),
                }
            }
            ControlCommand::SetOutputEnabled { enabled } => {
                match self.command(RuntimeCommand::ToggleOutput { enabled }) {
                    Ok(()) => ControlResponse::ack(request_id),
                    Err(error) => ControlResponse::error(request_id, error.to_string()),
                }
            }
            ControlCommand::ReloadModel => match self.command(RuntimeCommand::ReloadModel) {
                Ok(()) => ControlResponse::ack(request_id),
                Err(error) => ControlResponse::error(request_id, error.to_string()),
            },
            ControlCommand::PromoteCandidateModel => {
                match self.command(RuntimeCommand::PromoteCandidateModel) {
                    Ok(()) => ControlResponse::ack(request_id),
                    Err(error) => ControlResponse::error(request_id, error.to_string()),
                }
            }
            ControlCommand::RescanStreams => match self.command(RuntimeCommand::RescanStreams) {
                Ok(()) => ControlResponse::ack(request_id),
                Err(error) => ControlResponse::error(request_id, error.to_string()),
            },
            ControlCommand::ConnectStream { stream_id } => {
                match self.command(RuntimeCommand::ConnectStream { stream_id }) {
                    Ok(()) => ControlResponse::ack(request_id),
                    Err(error) => ControlResponse::error(request_id, error.to_string()),
                }
            }
            ControlCommand::DisconnectStream { stream_id } => {
                match self.command(RuntimeCommand::DisconnectStream { stream_id }) {
                    Ok(()) => ControlResponse::ack(request_id),
                    Err(error) => ControlResponse::error(request_id, error.to_string()),
                }
            }
            ControlCommand::SetLearningEnabled { enabled } => {
                match self.command(RuntimeCommand::SetLearningEnabled { enabled }) {
                    Ok(()) => ControlResponse::ack(request_id),
                    Err(error) => ControlResponse::error(request_id, error.to_string()),
                }
            }
            ControlCommand::MlBridgeReconnect => {
                match self.command(RuntimeCommand::MlBridgeReconnect) {
                    Ok(()) => ControlResponse::ack(request_id),
                    Err(error) => ControlResponse::error(request_id, error.to_string()),
                }
            }
            ControlCommand::TrainerSnapshot => {
                ControlResponse::trainer_snapshot(request_id, self.trainer_snapshot())
            }
            ControlCommand::SetFallbackPolicy { policy } => {
                match self.command(RuntimeCommand::SetFallbackPolicy { policy }) {
                    Ok(()) => ControlResponse::ack(request_id),
                    Err(error) => ControlResponse::error(request_id, error.to_string()),
                }
            }
            ControlCommand::SetSignalConfig { signal } => {
                match self.command(RuntimeCommand::SetSignalConfig { signal }) {
                    Ok(()) => ControlResponse::ack(request_id),
                    Err(error) => ControlResponse::error(request_id, error.to_string()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use neurohid_types::{
        config::{DeviceBackend, SystemConfig},
        control::{ControlCommand, ControlRequest, ControlResponsePayload},
    };

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

    #[tokio::test]
    async fn managed_runtime_dispatches_serialized_control_requests() {
        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::Mock;
        config.service.ipc_simulation_enabled = true;
        config.action.enabled = false;
        let mut updated_signal = config.signal.clone();
        updated_signal.notch_filter_enabled = !updated_signal.notch_filter_enabled;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");
        wait_for(Duration::from_secs(3), || runtime.snapshot().running).await;

        let snapshot_response = runtime.dispatch_control_request(ControlRequest {
            request_id: Some("snap-1".to_string()),
            command: ControlCommand::Snapshot,
        });
        assert_eq!(snapshot_response.request_id.as_deref(), Some("snap-1"));
        assert!(matches!(
            snapshot_response.payload,
            ControlResponsePayload::Snapshot { .. }
        ));

        let toggle_response = runtime.dispatch_control_request(ControlRequest {
            request_id: Some("set-output".to_string()),
            command: ControlCommand::SetOutputEnabled { enabled: false },
        });
        assert_eq!(toggle_response.request_id.as_deref(), Some("set-output"));
        assert_eq!(toggle_response.payload, ControlResponsePayload::Ack);
        wait_for(Duration::from_secs(1), || {
            !runtime.snapshot().output_enabled
        })
        .await;

        let signal_response = runtime.dispatch_control_request(ControlRequest {
            request_id: Some("set-signal".to_string()),
            command: ControlCommand::SetSignalConfig {
                signal: updated_signal,
            },
        });
        assert_eq!(signal_response.request_id.as_deref(), Some("set-signal"));
        assert_eq!(signal_response.payload, ControlResponsePayload::Ack);

        let stop_response = runtime.dispatch_control_request(ControlRequest {
            request_id: Some("shutdown".to_string()),
            command: ControlCommand::Shutdown,
        });
        assert_eq!(stop_response.payload, ControlResponsePayload::Ack);
        runtime.wait().await.expect("runtime should stop cleanly");
    }
}
