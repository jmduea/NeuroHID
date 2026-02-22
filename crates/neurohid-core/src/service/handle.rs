//! Service handle for non-blocking runtime control.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};

use neurohid_ipc::{IpcEnvelope, RuntimeEvent};
use neurohid_types::{
    action::Action,
    config::FallbackPolicy,
    error::Result,
    event::StreamMarker,
    profile::ProfileId,
    signal::{FeatureVector, Sample},
};

use super::{DecoderCommand, DeviceCommand, ServiceState, SignalCommand};
use crate::tasks::{RecordingRequest, TrainerIngressEvent};

/// A handle to a running service, returned by `NeuroHidService::spawn()`.
///
/// The handle lets the owner (e.g., the hub GUI) observe service state,
/// toggle calibration mode, and request shutdown — all without blocking.
///
/// Direct field access is an internal implementation detail. Embedders should
/// use [`crate::runtime::RuntimeHandle`] and its methods instead.
pub struct ServiceHandle {
    /// Shared service state — read with `try_read()` from the GUI thread.
    pub(crate) state: Arc<RwLock<ServiceState>>,

    /// Send `()` on this channel to request graceful shutdown.
    pub(crate) shutdown_tx: broadcast::Sender<()>,

    /// The spawned task's join handle. Await it to detect completion/panics.
    pub(crate) join_handle: tokio::task::JoinHandle<Result<()>>,

    /// Receiver for live EEG samples during calibration mode.
    /// Only produces values when `calibration_mode` is `true`.
    pub(crate) calibration_sample_rx: mpsc::Receiver<Sample>,

    /// Atomic flag to toggle calibration mode from the GUI thread.
    pub(crate) calibration_mode: Arc<AtomicBool>,

    /// Atomic flag to pause/resume HID output without restarting the service.
    pub(crate) output_enabled: Arc<AtomicBool>,

    /// Send commands to the DeviceTask (connect/disconnect/rescan).
    pub(crate) device_command_tx: mpsc::Sender<DeviceCommand>,

    /// Broadcast receiver for ALL live EEG samples (for visualization widgets).
    /// Unlike `calibration_sample_rx`, this always produces values.
    pub(crate) sample_broadcast_rx: broadcast::Receiver<Sample>,
    /// Broadcast sender for live EEG samples (for resubscribe-capable clones).
    pub(crate) sample_broadcast_tx: broadcast::Sender<Sample>,

    /// Broadcast receiver for extracted feature vectors (for visualization widgets).
    pub(crate) feature_broadcast_rx: broadcast::Receiver<FeatureVector>,
    /// Broadcast sender for extracted feature vectors (for resubscribe-capable clones).
    pub(crate) feature_broadcast_tx: broadcast::Sender<FeatureVector>,

    /// Broadcast receiver for decoded actions (for visualization widgets).
    pub(crate) action_broadcast_rx: broadcast::Receiver<Action>,
    /// Broadcast sender for decoded actions (for resubscribe-capable clones).
    pub(crate) action_broadcast_tx: broadcast::Sender<Action>,

    /// Broadcast receiver for marker/event annotations.
    pub(crate) marker_broadcast_rx: broadcast::Receiver<StreamMarker>,
    /// Broadcast sender for marker/event annotations (for resubscribe-capable clones).
    pub(crate) marker_broadcast_tx: broadcast::Sender<StreamMarker>,

    /// Send recording commands (start/stop) and receive result via oneshot in the request.
    pub(crate) recording_command_tx: mpsc::Sender<RecordingRequest>,

    /// Send commands to the SignalTask (e.g. runtime filter updates).
    pub(crate) signal_command_tx: mpsc::Sender<SignalCommand>,

    /// Send commands to the DecoderTask (reload model, switch profile).
    pub(crate) decoder_command_tx: mpsc::Sender<DecoderCommand>,

    /// In-process trainer ingress channel (transport -> IPC task protocol engine).
    pub(crate) trainer_ingress_tx: mpsc::Sender<TrainerIngressEvent>,

    /// In-process trainer egress channel (IPC task protocol engine -> transport).
    pub(crate) trainer_egress_rx: Arc<Mutex<mpsc::Receiver<IpcEnvelope>>>,

    /// Broadcast receiver for runtime bridge-derived events.
    pub(crate) runtime_event_broadcast_rx: broadcast::Receiver<RuntimeEvent>,

    /// Broadcast sender for runtime bridge-derived events (for resubscribe-capable clones).
    pub(crate) runtime_event_broadcast_tx: broadcast::Sender<RuntimeEvent>,
}

impl ServiceHandle {
    /// Toggle calibration mode and synchronize shared snapshot state.
    pub fn set_calibration_mode(&self, enabled: bool) {
        self.calibration_mode
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut state) = self.state.try_write() {
            state.calibration_mode = enabled;
        } else if tokio::runtime::Handle::try_current().is_err() {
            let mut state = self.state.blocking_write();
            state.calibration_mode = enabled;
        }
    }

    /// Toggle HID output without restarting the service.
    pub fn set_output_enabled(&self, enabled: bool) {
        self.output_enabled
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut state) = self.state.try_write() {
            state.output_enabled = enabled;
        } else if tokio::runtime::Handle::try_current().is_err() {
            let mut state = self.state.blocking_write();
            state.output_enabled = enabled;
        }
    }

    /// Toggle runtime learning state.
    pub fn set_learning_enabled(&self, enabled: bool) {
        if let Ok(mut state) = self.state.try_write() {
            state.learning_enabled = enabled;
        } else if tokio::runtime::Handle::try_current().is_err() {
            let mut state = self.state.blocking_write();
            state.learning_enabled = enabled;
        }
    }

    /// Request ML bridge reconnect.
    ///
    /// Current bridge loop reconnects automatically, so this clears the
    /// stale flag and lets the runtime re-enter fallback/full as telemetry updates.
    pub fn ml_bridge_reconnect(&self) {
        if let Ok(mut state) = self.state.try_write() {
            state.ml_bridge_stalled = false;
        } else if tokio::runtime::Handle::try_current().is_err() {
            let mut state = self.state.blocking_write();
            state.ml_bridge_stalled = false;
        }
    }

    /// Update fallback policy used by action capability gating.
    pub fn set_fallback_policy(&self, policy: FallbackPolicy) {
        if let Ok(mut state) = self.state.try_write() {
            state.fallback_policy = policy;
        } else if tokio::runtime::Handle::try_current().is_err() {
            let mut state = self.state.blocking_write();
            state.fallback_policy = policy;
        }
    }

    /// Last heartbeat timestamp reported by ML bridge.
    pub fn last_ml_heartbeat_us(&self) -> Option<i64> {
        if let Ok(state) = self.state.try_read() {
            return state.ml_bridge_last_heartbeat_us;
        }
        None
    }

    /// Request decoder model reload for the current active profile.
    pub fn reload_model(&self) {
        let _ = self
            .decoder_command_tx
            .try_send(DecoderCommand::ReloadModel);
    }

    /// Request candidate-model promotion with guardrail validation.
    pub fn promote_candidate_model(&self) {
        let _ = self
            .decoder_command_tx
            .try_send(DecoderCommand::PromoteCandidateModel);
    }

    /// Update active profile state used for action gating and model selection.
    pub fn set_profile_status(
        &self,
        profile_id: Option<ProfileId>,
        name: Option<String>,
        ready: bool,
    ) {
        let _ = self
            .decoder_command_tx
            .try_send(DecoderCommand::SetActiveProfile {
                profile_id: profile_id.clone(),
            });
        if let Ok(mut state) = self.state.try_write() {
            state.active_profile_name = name.clone();
            state.profile_ready = ready;
        } else if tokio::runtime::Handle::try_current().is_err() {
            let mut state = self.state.blocking_write();
            state.active_profile_name = name;
            state.profile_ready = ready;
        }
    }
}
