//! # NeuroHID Service
//!
//! This module contains the main service struct that orchestrates all the
//! concurrent tasks. Think of it as the "conductor" of an orchestra - it doesn't
//! play any instruments itself, but it makes sure everyone starts at the right
//! time, stays in sync, and stops together gracefully.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

use neurohid_storage::ProfileStore;
use neurohid_types::{
    IpcEnvelope,
    action::Action,
    config::{DecoderConfig, FallbackPolicy, LatencyAlertConfig, SignalConfig, SystemConfig},
    control::RuntimeModeState,
    device::DiscoveredStream,
    error::Result,
    event::StreamMarker,
    ipc::RuntimeEvent,
    profile::ProfileId,
    signal::{FeatureVector, Sample},
};

/// Commands sent from the hub to the DeviceTask for stream management.
#[derive(Debug)]
pub enum DeviceCommand {
    /// Re-scan for available LSL streams
    Rescan,
    /// Connect to a specific stream by its id
    Connect(String),
    /// Disconnect from a specific stream by its id
    Disconnect(String),
}

/// Commands sent from the hub to the SignalTask for live reconfiguration.
#[derive(Debug, Clone)]
pub enum SignalCommand {
    /// Replace the active signal-processing configuration at runtime.
    UpdateConfig(SignalConfig),
}

/// Runtime integrity stages tracked in the shared health rollup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrityStage {
    Device,
    Signal,
    Decoder,
    Action,
    Ipc,
}

#[derive(Debug, Clone, Copy)]
struct IntegrityStageMetrics {
    issues: u64,
    degraded: bool,
}

impl IntegrityStageMetrics {
    const fn ok() -> Self {
        Self {
            issues: 0,
            degraded: false,
        }
    }
}

/// Commands sent from the hub/runtime to the DecoderTask.
#[derive(Debug, Clone)]
pub enum DecoderCommand {
    /// Reload model artifacts for the currently active profile.
    ReloadModel,
    /// Promote validated candidate artifacts and hot-swap decoder model.
    PromoteCandidateModel,
    /// Switch active profile context and load its decoder model.
    SetActiveProfile { profile_id: Option<ProfileId> },
}

use crate::tasks::{
    ActionTask, DecisionEventRecord, DecoderTask, DeviceTask, EpisodeLogRecord, IpcTask,
    LatencyAlertMonitorTask, OutletTask, RecordingRequest, RecordingTask, SessionLoggerTask,
    SignalTask, TrainerIngressEvent,
};

/// The main service that coordinates all NeuroHID operations.
///
/// The service follows a "task supervisor" pattern: it spawns several
/// independent tasks that communicate via channels, and monitors them
/// for failures. If any critical task fails, the service initiates
/// a graceful shutdown.
pub struct NeuroHidService {
    /// System configuration
    config: SystemConfig,

    /// Profile storage handle retained for runtime profile operations.
    profile_store: Option<ProfileStore>,

    /// Active profile identifier retained for profile-aware task wiring.
    profile_id: Option<ProfileId>,

    /// Shutdown signal receiver
    shutdown_rx: broadcast::Receiver<()>,

    /// Shared state that tasks can read
    shared_state: Arc<RwLock<ServiceState>>,
}

/// Shared state accessible by all tasks.
///
/// This struct contains state that multiple tasks need to read. Write access
/// is carefully controlled to avoid contention. Most inter-task communication
/// happens through channels rather than shared state.
pub struct ServiceState {
    /// Whether the service is currently active (processing and emitting actions)
    pub active: bool,

    /// Whether online learning is enabled.
    pub learning_enabled: bool,

    /// Current signal quality (updated by device task)
    pub signal_quality: f32,

    /// Actions emitted since service start
    pub actions_emitted: u64,

    /// Errors detected since service start
    pub errors_detected: u64,

    /// Whether a device is currently connected
    pub device_connected: bool,

    /// Name of the connected device (if any)
    pub device_name: Option<String>,

    /// Battery level of the connected device (0-100)
    pub device_battery: Option<u8>,

    /// When the service was started
    pub started_at: Option<std::time::Instant>,

    /// Name of the active profile
    pub active_profile_name: Option<String>,

    /// Whether the active profile is calibrated and ready for HID emission.
    pub profile_ready: bool,

    /// Whether a compatible Rust runtime decoder model is loaded.
    pub decoder_ready: bool,

    /// Loaded decoder model version (if available).
    pub decoder_model_version: Option<String>,

    /// Whether the IPC bridge to Python is connected
    pub ipc_connected: bool,

    /// Whether IPC is currently running in simulated mode.
    pub ipc_simulated: bool,

    /// Whether the runtime ML bridge is currently connected.
    pub ml_bridge_connected: bool,

    /// Whether runtime ML bridge heartbeat is stale.
    pub ml_bridge_stalled: bool,

    /// Last runtime ML bridge heartbeat timestamp (micros).
    pub ml_bridge_last_heartbeat_us: Option<i64>,

    /// Effective protocol version for runtime ML bridge.
    pub ml_protocol_version: Option<u16>,

    /// Trainer replay size when reported by bridge.
    pub trainer_replay_size: Option<u64>,

    /// Trainer step when reported by bridge.
    pub trainer_step: Option<u64>,

    /// Trainer policy loss when reported by bridge.
    pub trainer_policy_loss: Option<f32>,

    /// Trainer value loss when reported by bridge.
    pub trainer_value_loss: Option<f32>,

    /// Trainer entropy when reported by bridge.
    pub trainer_entropy: Option<f32>,

    /// Last trainer-side status/error message.
    pub trainer_last_error: Option<String>,

    /// Count of candidate promotions accepted by runtime.
    pub candidate_promotions_succeeded: u64,

    /// Count of candidate promotions rejected by runtime guardrails/validation.
    pub candidate_promotions_rejected: u64,

    /// Last candidate promotion outcome message.
    pub candidate_last_outcome: Option<String>,

    /// Runtime mode classification for fallback/degraded behavior.
    pub runtime_mode_state: RuntimeModeState,

    /// Currently enabled action capabilities.
    pub enabled_capabilities: Vec<String>,

    /// Human-readable fallback/degraded capability message.
    pub limited_capabilities_message: Option<String>,

    /// Last timestamp when a runtime mode alert was emitted.
    pub last_runtime_mode_alert_us: Option<i64>,

    /// Current model kind used by decoder path (`onnx`, `lightweight_rust`, `none`).
    pub fallback_model_kind: Option<String>,

    /// Rolling success score derived from ErrP results.
    pub rolling_success_score: f32,

    /// Active fallback policy, mutable via control protocol.
    pub fallback_policy: FallbackPolicy,

    /// Whether the service is in calibration mode (pauses HID emission)
    pub calibration_mode: bool,

    /// Whether HID output is currently enabled.
    pub output_enabled: bool,

    /// Most recent decoder latency (feature extraction to decode output), in microseconds.
    pub decode_latency_last_us: u64,

    /// Rolling decoder latency p95, in microseconds.
    pub decode_latency_p95_us: u64,

    /// Most recent signal-stage latency (sample timestamp to extracted features), in microseconds.
    pub signal_latency_last_us: u64,

    /// Rolling signal-stage latency p95, in microseconds.
    pub signal_latency_p95_us: u64,

    /// Most recent end-to-end action latency (feature timestamp to HID emission), in microseconds.
    pub action_latency_last_us: u64,

    /// Rolling end-to-end action latency p95, in microseconds.
    pub action_latency_p95_us: u64,

    /// Whether runtime latency is currently in degraded state.
    pub latency_degraded: bool,

    /// Human-readable latency degradation summary.
    pub latency_alert_message: Option<String>,

    /// If a task failed at runtime, (task_name, error_message).
    /// Populated by `run_inner()` so the GUI can display what went wrong.
    pub task_error: Option<(String, String)>,

    /// LSL streams discovered on the network.
    /// Updated periodically by the DeviceTask.
    pub discovered_streams: Vec<DiscoveredStream>,

    /// Number of streams currently classified as EEG-routed.
    pub routed_eeg_streams: u64,

    /// Number of streams currently classified as motion-routed.
    pub routed_motion_streams: u64,

    /// Number of streams currently classified as auxiliary-routed.
    pub routed_auxiliary_streams: u64,

    /// Number of streams currently classified as unknown-routed.
    pub routed_unknown_streams: u64,

    /// Whether the runtime has entered a degraded integrity state.
    pub pipeline_integrity_degraded: bool,

    /// Count of integrity issues observed across pipeline stages.
    pub integrity_issue_count: u64,

    /// Human-readable stage health summary.
    pub stage_health_summary: Option<String>,

    /// Whether session recording is currently active.
    pub recording_active: bool,
    /// Session id of the current recording, if any.
    pub current_session_id: Option<String>,
    // Internal per-stage integrity rollup state.
    integrity_device: IntegrityStageMetrics,
    integrity_signal: IntegrityStageMetrics,
    integrity_decoder: IntegrityStageMetrics,
    integrity_action: IntegrityStageMetrics,
    integrity_ipc: IntegrityStageMetrics,
    integrity_signal_eeg_streams_total: u64,
    integrity_signal_eeg_streams_degraded: u64,
}

impl Default for ServiceState {
    fn default() -> Self {
        Self {
            active: false,
            learning_enabled: true,
            signal_quality: 0.0,
            actions_emitted: 0,
            errors_detected: 0,
            device_connected: false,
            device_name: None,
            device_battery: None,
            started_at: None,
            active_profile_name: None,
            profile_ready: false,
            decoder_ready: false,
            decoder_model_version: None,
            ipc_connected: false,
            ipc_simulated: false,
            ml_bridge_connected: false,
            ml_bridge_stalled: false,
            ml_bridge_last_heartbeat_us: None,
            ml_protocol_version: None,
            trainer_replay_size: None,
            trainer_step: None,
            trainer_policy_loss: None,
            trainer_value_loss: None,
            trainer_entropy: None,
            trainer_last_error: None,
            candidate_promotions_succeeded: 0,
            candidate_promotions_rejected: 0,
            candidate_last_outcome: None,
            runtime_mode_state: RuntimeModeState::Degraded,
            enabled_capabilities: Vec::new(),
            limited_capabilities_message: None,
            last_runtime_mode_alert_us: None,
            fallback_model_kind: None,
            rolling_success_score: 1.0,
            fallback_policy: FallbackPolicy::default(),
            calibration_mode: false,
            output_enabled: true,
            decode_latency_last_us: 0,
            decode_latency_p95_us: 0,
            signal_latency_last_us: 0,
            signal_latency_p95_us: 0,
            action_latency_last_us: 0,
            action_latency_p95_us: 0,
            latency_degraded: false,
            latency_alert_message: None,
            task_error: None,
            discovered_streams: Vec::new(),
            routed_eeg_streams: 0,
            routed_motion_streams: 0,
            routed_auxiliary_streams: 0,
            routed_unknown_streams: 0,
            pipeline_integrity_degraded: false,
            integrity_issue_count: 0,
            stage_health_summary: None,
            recording_active: false,
            current_session_id: None,
            integrity_device: IntegrityStageMetrics::ok(),
            integrity_signal: IntegrityStageMetrics::ok(),
            integrity_decoder: IntegrityStageMetrics::ok(),
            integrity_action: IntegrityStageMetrics::ok(),
            integrity_ipc: IntegrityStageMetrics::ok(),
            integrity_signal_eeg_streams_total: 0,
            integrity_signal_eeg_streams_degraded: 0,
        }
    }
}

impl ServiceState {
    const INTEGRITY_CRITICAL_ISSUES_THRESHOLD: u64 = 25;

    fn stage_metrics_mut(&mut self, stage: IntegrityStage) -> &mut IntegrityStageMetrics {
        match stage {
            IntegrityStage::Device => &mut self.integrity_device,
            IntegrityStage::Signal => &mut self.integrity_signal,
            IntegrityStage::Decoder => &mut self.integrity_decoder,
            IntegrityStage::Action => &mut self.integrity_action,
            IntegrityStage::Ipc => &mut self.integrity_ipc,
        }
    }

    pub fn reset_integrity_rollup(&mut self) {
        self.integrity_device = IntegrityStageMetrics::ok();
        self.integrity_signal = IntegrityStageMetrics::ok();
        self.integrity_decoder = IntegrityStageMetrics::ok();
        self.integrity_action = IntegrityStageMetrics::ok();
        self.integrity_ipc = IntegrityStageMetrics::ok();
        self.integrity_signal_eeg_streams_total = 0;
        self.integrity_signal_eeg_streams_degraded = 0;
        self.recompute_integrity_rollup();
    }

    pub fn register_integrity_issue(&mut self, stage: IntegrityStage, critical: bool) {
        let metrics = self.stage_metrics_mut(stage);
        metrics.issues = metrics.issues.saturating_add(1);
        metrics.degraded = metrics.degraded || critical || metrics.issues > 0;
        self.recompute_integrity_rollup();
    }

    pub fn set_stage_integrity_snapshot(
        &mut self,
        stage: IntegrityStage,
        issues: u64,
        degraded: bool,
    ) {
        let metrics = self.stage_metrics_mut(stage);
        metrics.issues = issues;
        metrics.degraded = degraded || issues > 0;
        self.recompute_integrity_rollup();
    }

    pub fn set_signal_integrity_snapshot(
        &mut self,
        issues: u64,
        eeg_streams_total: u64,
        eeg_streams_degraded: u64,
    ) {
        self.integrity_signal.issues = issues;
        self.integrity_signal.degraded = issues > 0 || eeg_streams_degraded > 0;
        self.integrity_signal_eeg_streams_total = eeg_streams_total;
        self.integrity_signal_eeg_streams_degraded = eeg_streams_degraded;
        self.recompute_integrity_rollup();
    }

    fn stage_status(metrics: IntegrityStageMetrics) -> &'static str {
        if metrics.degraded { "degraded" } else { "ok" }
    }

    fn recompute_integrity_rollup(&mut self) {
        self.integrity_issue_count = self
            .integrity_device
            .issues
            .saturating_add(self.integrity_signal.issues)
            .saturating_add(self.integrity_decoder.issues)
            .saturating_add(self.integrity_action.issues)
            .saturating_add(self.integrity_ipc.issues);

        let all_eeg_impacted = self.integrity_signal_eeg_streams_total > 0
            && self.integrity_signal_eeg_streams_degraded
                >= self.integrity_signal_eeg_streams_total;
        let repeated_critical =
            self.integrity_issue_count >= Self::INTEGRITY_CRITICAL_ISSUES_THRESHOLD;
        self.pipeline_integrity_degraded = all_eeg_impacted || repeated_critical;

        let pipeline_status = if self.pipeline_integrity_degraded {
            "degraded"
        } else {
            "ok"
        };
        let pipeline_reason = if all_eeg_impacted {
            format!(
                "all_eeg_impacted({}/{})",
                self.integrity_signal_eeg_streams_degraded, self.integrity_signal_eeg_streams_total
            )
        } else if repeated_critical {
            format!(
                "critical_threshold({}/{})",
                self.integrity_issue_count,
                Self::INTEGRITY_CRITICAL_ISSUES_THRESHOLD
            )
        } else {
            "normal".to_string()
        };
        self.stage_health_summary = Some(format!(
            "pipeline:{pipeline_status}[{pipeline_reason}] \
             device:{}({}) signal:{}({}) decoder:{}({}) action:{}({}) ipc:{}({})",
            Self::stage_status(self.integrity_device),
            self.integrity_device.issues,
            Self::stage_status(self.integrity_signal),
            self.integrity_signal.issues,
            Self::stage_status(self.integrity_decoder),
            self.integrity_decoder.issues,
            Self::stage_status(self.integrity_action),
            self.integrity_action.issues,
            Self::stage_status(self.integrity_ipc),
            self.integrity_ipc.issues
        ));
    }
}

async fn resolve_profile_status(
    profile_store: Option<&ProfileStore>,
    profile_id: Option<&ProfileId>,
) -> (Option<String>, bool) {
    let Some(profile_id) = profile_id else {
        return (None, false);
    };
    let Some(store) = profile_store else {
        return (Some(profile_id.to_string()), false);
    };

    match store.get_metadata(profile_id).await {
        Ok(metadata) => (Some(metadata.name), metadata.calibration_state.is_ready()),
        Err(err) => {
            tracing::warn!(
                "Failed to resolve profile metadata for {}: {}",
                profile_id,
                err
            );
            (Some(profile_id.to_string()), false)
        }
    }
}

/// A handle to a running service, returned by `NeuroHidService::spawn()`.
///
/// The handle lets the owner (e.g., the hub GUI) observe service state,
/// toggle calibration mode, and request shutdown — all without blocking.
pub struct ServiceHandle {
    /// Shared service state — read with `try_read()` from the GUI thread.
    pub state: Arc<RwLock<ServiceState>>,

    /// Send `()` on this channel to request graceful shutdown.
    pub shutdown_tx: broadcast::Sender<()>,

    /// The spawned task's join handle. Await it to detect completion/panics.
    pub join_handle: tokio::task::JoinHandle<Result<()>>,

    /// Receiver for live EEG samples during calibration mode.
    /// Only produces values when `calibration_mode` is `true`.
    pub calibration_sample_rx: mpsc::Receiver<Sample>,

    /// Atomic flag to toggle calibration mode from the GUI thread.
    pub calibration_mode: Arc<AtomicBool>,

    /// Atomic flag to pause/resume HID output without restarting the service.
    pub output_enabled: Arc<AtomicBool>,

    /// Send commands to the DeviceTask (connect/disconnect/rescan).
    pub device_command_tx: mpsc::Sender<DeviceCommand>,

    /// Broadcast receiver for ALL live EEG samples (for visualization widgets).
    /// Unlike `calibration_sample_rx`, this always produces values.
    pub sample_broadcast_rx: broadcast::Receiver<Sample>,
    /// Broadcast sender for live EEG samples (for resubscribe-capable clones).
    pub sample_broadcast_tx: broadcast::Sender<Sample>,

    /// Broadcast receiver for extracted feature vectors (for visualization widgets).
    pub feature_broadcast_rx: broadcast::Receiver<FeatureVector>,
    /// Broadcast sender for extracted feature vectors (for resubscribe-capable clones).
    pub feature_broadcast_tx: broadcast::Sender<FeatureVector>,

    /// Broadcast receiver for decoded actions (for visualization widgets).
    pub action_broadcast_rx: broadcast::Receiver<Action>,
    /// Broadcast sender for decoded actions (for resubscribe-capable clones).
    pub action_broadcast_tx: broadcast::Sender<Action>,

    /// Broadcast receiver for marker/event annotations.
    pub marker_broadcast_rx: broadcast::Receiver<StreamMarker>,
    /// Broadcast sender for marker/event annotations (for resubscribe-capable clones).
    pub marker_broadcast_tx: broadcast::Sender<StreamMarker>,

    /// Send recording commands (start/stop) and receive result via oneshot in the request.
    pub recording_command_tx: mpsc::Sender<RecordingRequest>,

    /// Send commands to the SignalTask (e.g. runtime filter updates).
    pub signal_command_tx: mpsc::Sender<SignalCommand>,

    /// Send commands to the DecoderTask (reload model, switch profile).
    pub decoder_command_tx: mpsc::Sender<DecoderCommand>,

    /// In-process trainer ingress channel (transport -> IPC task protocol engine).
    pub trainer_ingress_tx: mpsc::Sender<TrainerIngressEvent>,

    /// In-process trainer egress channel (IPC task protocol engine -> transport).
    pub trainer_egress_rx: Arc<Mutex<mpsc::Receiver<IpcEnvelope>>>,

    /// Broadcast receiver for runtime bridge-derived events.
    pub runtime_event_broadcast_rx: broadcast::Receiver<RuntimeEvent>,

    /// Broadcast sender for runtime bridge-derived events (for resubscribe-capable clones).
    pub runtime_event_broadcast_tx: broadcast::Sender<RuntimeEvent>,
}

impl NeuroHidService {
    /// Creates a new NeuroHID service.
    ///
    /// This initializes all the components but doesn't start any tasks yet.
    /// Call `run()` to start the service (blocking) or `spawn()` to start
    /// it in the background and get a `ServiceHandle`.
    pub async fn new(
        config: SystemConfig,
        profile_store: Option<ProfileStore>,
        profile_id: Option<ProfileId>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<Self> {
        let shared_state = Arc::new(RwLock::new(ServiceState::default()));

        let (active_profile_name, profile_ready) =
            resolve_profile_status(profile_store.as_ref(), profile_id.as_ref()).await;
        {
            let mut state = shared_state.write().await;
            state.active_profile_name = active_profile_name;
            state.profile_ready = profile_ready;
            state.output_enabled = config.action.enabled;
            state.fallback_policy = config.service.fallback_policy.clone();
        }

        Ok(Self {
            config,
            profile_store,
            profile_id,
            shutdown_rx,
            shared_state,
        })
    }

    /// Spawns the service on the tokio runtime and returns a non-blocking handle.
    ///
    /// This is the preferred entry point when embedding the service inside a GUI.
    /// The GUI can read state via `handle.state`, toggle calibration via
    /// `handle.calibration_mode`, and shut down via `handle.shutdown_tx`.
    pub fn spawn(self, shutdown_tx: broadcast::Sender<()>) -> ServiceHandle {
        let state = Arc::clone(&self.shared_state);
        let calibration_flag = Arc::new(AtomicBool::new(false));
        let calibration_flag_clone = Arc::clone(&calibration_flag);
        let output_flag = Arc::new(AtomicBool::new(self.config.action.enabled));
        let output_flag_clone = Arc::clone(&output_flag);

        // Channel for forwarding live samples to the calibration panel.
        // Bounded to 256 to avoid unbounded growth if the panel falls behind.
        let (cal_sample_tx, cal_sample_rx) = mpsc::channel::<Sample>(256);

        // Channel for stream management commands from the GUI.
        let (device_cmd_tx, device_cmd_rx) = mpsc::channel::<DeviceCommand>(16);
        // Channel for runtime signal reconfiguration from the GUI.
        let (signal_cmd_tx, signal_cmd_rx) = mpsc::channel::<SignalCommand>(16);
        // Channel for runtime decoder control commands from GUI/runtime API.
        let (decoder_cmd_tx, decoder_cmd_rx) = mpsc::channel::<DecoderCommand>(16);
        let decoder_cmd_tx_for_run_inner = decoder_cmd_tx.clone();
        // Channel for recording start/stop (request + oneshot reply).
        let (recording_cmd_tx, recording_cmd_rx) = mpsc::channel::<RecordingRequest>(16);

        // In-process trainer bridge channels (transport owned by service binary).
        let (trainer_ingress_tx, trainer_ingress_rx) = mpsc::channel::<TrainerIngressEvent>(1024);
        let (trainer_egress_tx, trainer_egress_rx) = mpsc::channel::<IpcEnvelope>(1024);

        // Broadcast channels for live data visualization in the hub.
        // These fan-out to multiple widget subscribers.
        let (sample_broadcast_tx, sample_broadcast_rx) = broadcast::channel::<Sample>(512);
        let (feature_broadcast_tx, feature_broadcast_rx) = broadcast::channel::<FeatureVector>(128);
        let (action_broadcast_tx, action_broadcast_rx) = broadcast::channel::<Action>(128);
        let (marker_broadcast_tx, marker_broadcast_rx) = broadcast::channel::<StreamMarker>(256);
        let (runtime_event_broadcast_tx, runtime_event_broadcast_rx) =
            broadcast::channel::<RuntimeEvent>(512);
        let sample_broadcast_tx_for_handle = sample_broadcast_tx.clone();
        let feature_broadcast_tx_for_handle = feature_broadcast_tx.clone();
        let action_broadcast_tx_for_handle = action_broadcast_tx.clone();
        let marker_broadcast_tx_for_handle = marker_broadcast_tx.clone();
        let runtime_event_broadcast_tx_for_handle = runtime_event_broadcast_tx.clone();

        let join_handle = tokio::spawn(async move {
            self.run_inner(
                Some(calibration_flag_clone),
                Some(output_flag_clone),
                Some(cal_sample_tx),
                Some(device_cmd_rx),
                Some(signal_cmd_rx),
                Some(decoder_cmd_tx_for_run_inner),
                Some(decoder_cmd_rx),
                Some(sample_broadcast_tx),
                Some(feature_broadcast_tx),
                Some(action_broadcast_tx),
                Some(marker_broadcast_tx),
                Some(trainer_ingress_rx),
                Some(trainer_egress_tx),
                Some(runtime_event_broadcast_tx),
                Some(recording_cmd_rx),
            )
            .await
        });

        ServiceHandle {
            state,
            shutdown_tx,
            join_handle,
            calibration_sample_rx: cal_sample_rx,
            calibration_mode: calibration_flag,
            output_enabled: output_flag,
            device_command_tx: device_cmd_tx,
            sample_broadcast_rx,
            sample_broadcast_tx: sample_broadcast_tx_for_handle,
            feature_broadcast_rx,
            feature_broadcast_tx: feature_broadcast_tx_for_handle,
            action_broadcast_rx,
            action_broadcast_tx: action_broadcast_tx_for_handle,
            marker_broadcast_rx,
            marker_broadcast_tx: marker_broadcast_tx_for_handle,
            recording_command_tx: recording_cmd_tx,
            signal_command_tx: signal_cmd_tx,
            decoder_command_tx: decoder_cmd_tx,
            trainer_ingress_tx,
            trainer_egress_rx: Arc::new(Mutex::new(trainer_egress_rx)),
            runtime_event_broadcast_rx,
            runtime_event_broadcast_tx: runtime_event_broadcast_tx_for_handle,
        }
    }

    /// Runs the service until shutdown is requested (blocking).
    ///
    /// This is the entry point for the standalone headless binary.
    pub async fn run(self) -> Result<()> {
        let (decoder_cmd_tx, decoder_cmd_rx) = mpsc::channel::<DecoderCommand>(16);
        self.run_inner(
            None,
            None,
            None,
            None,
            None,
            Some(decoder_cmd_tx),
            Some(decoder_cmd_rx),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
    }

    /// Internal run loop shared by both `run()` and `spawn()`.
    #[expect(
        clippy::too_many_arguments,
        reason = "Service bootstrap needs explicit channel/flag wiring for embedded and headless modes"
    )]
    async fn run_inner(
        mut self,
        calibration_flag: Option<Arc<AtomicBool>>,
        output_enabled_flag: Option<Arc<AtomicBool>>,
        calibration_sample_tx: Option<mpsc::Sender<Sample>>,
        device_command_rx: Option<mpsc::Receiver<DeviceCommand>>,
        signal_command_rx: Option<mpsc::Receiver<SignalCommand>>,
        decoder_command_tx: Option<mpsc::Sender<DecoderCommand>>,
        decoder_command_rx: Option<mpsc::Receiver<DecoderCommand>>,
        sample_broadcast_tx: Option<broadcast::Sender<Sample>>,
        feature_broadcast_tx: Option<broadcast::Sender<FeatureVector>>,
        action_broadcast_tx: Option<broadcast::Sender<Action>>,
        marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,
        trainer_ingress_rx: Option<mpsc::Receiver<TrainerIngressEvent>>,
        trainer_egress_tx: Option<mpsc::Sender<IpcEnvelope>>,
        runtime_event_broadcast_tx: Option<broadcast::Sender<RuntimeEvent>>,
        recording_command_rx: Option<mpsc::Receiver<RecordingRequest>>,
    ) -> Result<()> {
        tracing::info!("Starting service tasks");

        // Create channels for inter-task communication.
        // These channels form the "nervous system" of the service.

        // Samples flow from device task to signal task
        let (sample_tx, sample_rx) = mpsc::channel(256);

        // Features flow from signal task to decoder task.
        let (feature_tx, feature_rx) = mpsc::channel(64);
        // Decision events are forwarded to the ML bridge from decoder output.
        let (decision_event_tx, decision_event_rx) = mpsc::channel::<DecisionEventRecord>(64);

        // Actions flow from decoder task to action task.
        let (action_tx, action_rx) = mpsc::channel(64);

        // Runtime episodes flow from decoder to session logger.
        let session_logging_enabled =
            self.config.storage.session_logging_enabled && self.profile_store.is_some();
        let (episode_log_tx, episode_log_rx) = if session_logging_enabled {
            let (tx, rx) = mpsc::channel::<EpisodeLogRecord>(256);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        // ErrP results flow from IPC back to signal task (for online learning coordination)
        let (errp_tx, errp_rx) = mpsc::channel(64);
        // Raw samples are mirrored into IPC for runtime-generated ErrP windows.
        let (errp_sample_tx, errp_sample_rx) = mpsc::channel(1024);

        // Clone shared state for each task
        let state_device = Arc::clone(&self.shared_state);
        let state_signal = Arc::clone(&self.shared_state);
        let state_decoder = Arc::clone(&self.shared_state);
        let state_ipc = Arc::clone(&self.shared_state);
        let state_action = Arc::clone(&self.shared_state);
        let state_latency = Arc::clone(&self.shared_state);

        // Clone shutdown receiver for each task (broadcast channels support multiple receivers)
        let shutdown_device = self.shutdown_rx.resubscribe();
        let shutdown_signal = self.shutdown_rx.resubscribe();
        let shutdown_decoder = self.shutdown_rx.resubscribe();
        let shutdown_ipc = self.shutdown_rx.resubscribe();
        let shutdown_action = self.shutdown_rx.resubscribe();
        let shutdown_outlet = self.shutdown_rx.resubscribe();
        let shutdown_latency_monitor = self.shutdown_rx.resubscribe();

        let marker_tx_for_signal = marker_broadcast_tx.clone();
        let marker_tx_for_ipc = marker_broadcast_tx.clone();
        let marker_tx_for_action = marker_broadcast_tx;
        let observability_config = self.config.service.observability.clone();

        // Optional outlet fan-out task: subscribes to the same broadcast channels
        // used by hub widgets and republishes to configured network targets.
        let outlet_config = self.config.outlet.clone();
        let outlet_sample_rx = sample_broadcast_tx.as_ref().map(|tx| tx.subscribe());
        let outlet_feature_rx = feature_broadcast_tx.as_ref().map(|tx| tx.subscribe());
        let outlet_action_rx = action_broadcast_tx.as_ref().map(|tx| tx.subscribe());
        let outlet_marker_rx = marker_tx_for_action.as_ref().map(|tx| tx.subscribe());
        let mut outlet_handle = if outlet_config.enabled {
            Some(tokio::spawn(async move {
                tracing::info!("Outlet task starting");
                let task = OutletTask::new(
                    outlet_config,
                    outlet_sample_rx,
                    outlet_feature_rx,
                    outlet_action_rx,
                    outlet_marker_rx,
                );
                task.run(shutdown_outlet).await
            }))
        } else {
            None
        };

        // Optional latency monitor task for sustained p95 threshold alerts.
        let latency_alert_config: LatencyAlertConfig = self.config.service.latency_alert.clone();
        let mut latency_monitor_handle = if latency_alert_config.enabled {
            Some(tokio::spawn(async move {
                tracing::info!("Latency monitor task starting");
                let task = LatencyAlertMonitorTask::new(latency_alert_config, state_latency);
                task.run(shutdown_latency_monitor).await
            }))
        } else {
            None
        };

        // Optional session logger for continuous-learning episode capture.
        let storage_config = self.config.storage.clone();
        let profile_store_for_session_logger = self.profile_store.clone();
        let shutdown_session_logger = self.shutdown_rx.resubscribe();
        let mut session_logger_handle = episode_log_rx.map(|episode_log_rx| {
            tokio::spawn(async move {
                tracing::info!("Session logger task starting");
                let task = SessionLoggerTask::new(
                    storage_config,
                    profile_store_for_session_logger,
                    episode_log_rx,
                );
                task.run(shutdown_session_logger).await
            })
        });

        // Optional recording task: subscribes to sample/action broadcast, writes session folder.
        let recording_config = self.config.recording.clone();
        let system_config_for_recording = self.config.clone();
        let profile_id_for_recording = self.profile_id.clone();
        let profile_store_for_recording = self.profile_store.clone();
        let state_for_recording = Arc::clone(&self.shared_state);
        let shutdown_recording = self.shutdown_rx.resubscribe();
        let mut recording_handle = recording_command_rx.and_then(|recording_cmd_rx| {
            let sample_rx = sample_broadcast_tx.as_ref()?.subscribe();
            let action_rx = action_broadcast_tx.as_ref()?.subscribe();
            Some(tokio::spawn(async move {
                tracing::info!("Recording task starting");
                let task = RecordingTask::new(
                    recording_config,
                    system_config_for_recording,
                    profile_id_for_recording,
                    profile_store_for_recording,
                    state_for_recording,
                    sample_rx,
                    action_rx,
                    recording_cmd_rx,
                );
                task.run(shutdown_recording).await
            }))
        });

        // Spawn the device task. This connects to the EEG device and streams
        // samples into the sample channel.
        let device_config = self.config.device.clone();
        let cal_tx_for_device = calibration_sample_tx.clone();
        let cal_flag_for_device = calibration_flag.as_ref().map(Arc::clone);
        let device_observability = observability_config.clone();
        let mut device_handle = tokio::spawn(async move {
            tracing::info!("Device task starting");
            let task = DeviceTask::new(
                device_config,
                sample_tx,
                state_device,
                cal_tx_for_device,
                cal_flag_for_device,
                device_command_rx,
                device_observability,
            );
            task.run(shutdown_device).await
        });

        // Spawn the signal processing task. This reads samples, applies filters,
        // extracts features, and sends them to the IPC channel.
        let signal_config = self.config.signal.clone();
        let signal_observability = observability_config.clone();
        let mut signal_handle = tokio::spawn(async move {
            tracing::info!("Signal task starting");
            let task = SignalTask::new(
                signal_config,
                sample_rx,
                feature_tx,
                errp_rx,
                state_signal,
                signal_command_rx,
                Some(errp_sample_tx),
                sample_broadcast_tx,
                feature_broadcast_tx,
                marker_tx_for_signal,
                signal_observability,
            );
            task.run(shutdown_signal).await
        });

        // Spawn the Rust decoder task. This performs ONNX inference in-process
        // so the signal->action path does not depend on Python bridge latency.
        let decoder_config: DecoderConfig = self.config.decoder.clone();
        let profile_store_for_decoder = self.profile_store.clone();
        let profile_id_for_decoder = self.profile_id.clone();
        let fallback_enabled = self.config.service.fallback_policy.enabled;
        let decoder_observability = observability_config.clone();
        let mut decoder_handle = tokio::spawn(async move {
            tracing::info!("Decoder task starting");
            let task = DecoderTask::new(
                decoder_config,
                feature_rx,
                action_tx,
                state_decoder,
                profile_store_for_decoder,
                profile_id_for_decoder,
                decoder_command_rx,
                Some(decision_event_tx),
                episode_log_tx,
                fallback_enabled,
                decoder_observability,
            );
            task.run(shutdown_decoder).await
        });

        // Spawn the IPC task. This remains available for Python-side ErrP and
        // training workflows, but action emission is handled by DecoderTask.
        let ipc_config = self.config.service.clone();
        let profile_store_for_ipc = self.profile_store.clone();
        let decoder_command_tx_for_ipc = decoder_command_tx.clone();
        let ipc_observability = observability_config.clone();
        let trainer_ingress_rx_for_ipc = trainer_ingress_rx.unwrap_or_else(|| {
            let (_tx, rx) = mpsc::channel::<TrainerIngressEvent>(1);
            rx
        });
        let trainer_egress_tx_for_ipc = trainer_egress_tx.unwrap_or_else(|| {
            let (tx, _rx) = mpsc::channel::<IpcEnvelope>(1);
            tx
        });
        let mut ipc_handle = tokio::spawn(async move {
            tracing::info!("IPC task starting");
            let task = IpcTask::new(
                ipc_config,
                decision_event_rx,
                errp_tx,
                errp_sample_rx,
                trainer_ingress_rx_for_ipc,
                trainer_egress_tx_for_ipc,
                self.config.errp.clone(),
                state_ipc,
                marker_tx_for_ipc,
                profile_store_for_ipc,
                decoder_command_tx_for_ipc,
                runtime_event_broadcast_tx,
                ipc_observability,
            );
            task.run(shutdown_ipc).await
        });

        // Spawn the action task. This takes decoded actions and emits them
        // as HID events (mouse movements, clicks, keystrokes).
        let action_config = self.config.action.clone();
        let cal_flag_for_action = calibration_flag.as_ref().map(Arc::clone);
        let output_flag_for_action = output_enabled_flag.as_ref().map(Arc::clone);
        let action_observability = observability_config;
        let mut action_handle = tokio::spawn(async move {
            tracing::info!("Action task starting");
            let task = ActionTask::new(
                action_config,
                action_rx,
                state_action,
                cal_flag_for_action,
                output_flag_for_action,
                action_broadcast_tx,
                marker_tx_for_action,
                action_observability,
            );
            task.run(shutdown_action).await
        });

        // Mark the service as active and clear any previous task error
        {
            let mut state = self.shared_state.write().await;
            state.active = true;
            state.started_at = Some(std::time::Instant::now());
            state.calibration_mode = calibration_flag
                .as_ref()
                .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed));
            state.output_enabled = output_enabled_flag
                .as_ref()
                .map_or(self.config.action.enabled, |flag| {
                    flag.load(std::sync::atomic::Ordering::Relaxed)
                });
            state.decoder_ready = false;
            state.decoder_model_version = None;
            state.decode_latency_last_us = 0;
            state.decode_latency_p95_us = 0;
            state.signal_latency_last_us = 0;
            state.signal_latency_p95_us = 0;
            state.action_latency_last_us = 0;
            state.action_latency_p95_us = 0;
            state.latency_degraded = false;
            state.latency_alert_message = None;
            state.ipc_connected = false;
            state.ipc_simulated = false;
            state.ml_bridge_connected = false;
            state.ml_bridge_stalled = false;
            state.ml_bridge_last_heartbeat_us = None;
            state.ml_protocol_version = Some(3);
            state.trainer_replay_size = None;
            state.trainer_step = None;
            state.trainer_policy_loss = None;
            state.trainer_value_loss = None;
            state.trainer_entropy = None;
            state.trainer_last_error = None;
            state.candidate_promotions_succeeded = 0;
            state.candidate_promotions_rejected = 0;
            state.candidate_last_outcome = None;
            state.runtime_mode_state = RuntimeModeState::Degraded;
            state.enabled_capabilities.clear();
            state.limited_capabilities_message = None;
            state.last_runtime_mode_alert_us = None;
            state.fallback_model_kind = None;
            state.rolling_success_score = 1.0;
            state.fallback_policy = self.config.service.fallback_policy.clone();
            state.task_error = None;
            state.reset_integrity_rollup();
        }

        tracing::info!("All tasks started, service is active");

        // Wait for shutdown signal or CRITICAL task failure.
        //
        // Tasks are classified into two tiers:
        //   - Critical (device, signal, decoder): these form the real-time
        //     acquisition->decode pipeline. If any fails, we must shut down.
        //   - Non-critical (ipc, action): failures here degrade functionality
        //     (e.g. no HID output) but the data pipeline keeps flowing so the
        //     console and visualizations continue to work.
        //
        // We use a loop + select with guards so that non-critical task exits
        // are recorded as warnings without breaking out of the loop.
        let mut task_failure: Option<(String, String)> = None;
        let mut ipc_done = false;
        let mut action_done = false;
        let mut latency_monitor_done = latency_monitor_handle.is_none();
        let mut session_logger_done = session_logger_handle.is_none();
        let mut recording_done = recording_handle.is_none();

        loop {
            // Non-critical optional tasks are tracked via `is_finished()` to avoid
            // guarding `select!` branches with `.expect(...)` on `Option<JoinHandle<_>>`.
            if !latency_monitor_done {
                if let Some(handle) = latency_monitor_handle.as_mut() {
                    if handle.is_finished() {
                        latency_monitor_done = true;
                        match handle.await {
                            Ok(Ok(())) => tracing::info!("Latency monitor task completed"),
                            Ok(Err(e)) => {
                                tracing::warn!("Latency monitor task failed (non-critical): {}", e);
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error = Some((
                                        "latency_monitor".into(),
                                        format!("{} — latency alerts disabled", e),
                                    ));
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Latency monitor task panicked (non-critical): {}",
                                    e
                                );
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error = Some((
                                        "latency_monitor".into(),
                                        format!("panicked: {}", e),
                                    ));
                                }
                            }
                        }
                    }
                } else {
                    latency_monitor_done = true;
                }
            }

            if !session_logger_done {
                if let Some(handle) = session_logger_handle.as_mut() {
                    if handle.is_finished() {
                        session_logger_done = true;
                        match handle.await {
                            Ok(Ok(())) => tracing::info!("Session logger task completed"),
                            Ok(Err(e)) => {
                                tracing::warn!("Session logger task failed (non-critical): {}", e);
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error = Some((
                                        "session_logger".into(),
                                        format!("{} — session logging disabled", e),
                                    ));
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Session logger task panicked (non-critical): {}",
                                    e
                                );
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error =
                                        Some(("session_logger".into(), format!("panicked: {}", e)));
                                }
                            }
                        }
                    }
                } else {
                    session_logger_done = true;
                }
            }

            if !recording_done {
                if let Some(handle) = recording_handle.as_mut() {
                    if handle.is_finished() {
                        recording_done = true;
                        match handle.await {
                            Ok(Ok(())) => tracing::info!("Recording task completed"),
                            Ok(Err(e)) => {
                                tracing::warn!("Recording task failed (non-critical): {}", e);
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error = Some((
                                        "recording".into(),
                                        format!("{} — recording disabled", e),
                                    ));
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Recording task panicked (non-critical): {}",
                                    e
                                );
                                let mut state = self.shared_state.write().await;
                                if state.task_error.is_none() {
                                    state.task_error =
                                        Some(("recording".into(), format!("panicked: {}", e)));
                                }
                            }
                        }
                    }
                } else {
                    recording_done = true;
                }
            }

            tokio::select! {
                // Shutdown signal received (user-initiated, no error)
                _ = self.shutdown_rx.recv() => {
                    tracing::info!("Shutdown signal received, stopping tasks");
                    break;
                }

                // ── Critical tasks ──────────────────────────────────────

                // Device task finished (either error or clean shutdown)
                result = &mut device_handle => {
                    match result {
                        Ok(Ok(())) => tracing::info!("Device task completed"),
                        Ok(Err(e)) => {
                            tracing::error!("Device task failed: {}", e);
                            task_failure = Some(("device".into(), e.to_string()));
                        }
                        Err(e) => {
                            tracing::error!("Device task panicked: {}", e);
                            task_failure = Some(("device".into(), e.to_string()));
                        }
                    }
                    break;
                }

                // Signal task finished
                result = &mut signal_handle => {
                    match result {
                        Ok(Ok(())) => tracing::info!("Signal task completed"),
                        Ok(Err(e)) => {
                            tracing::error!("Signal task failed: {}", e);
                            task_failure = Some(("signal".into(), e.to_string()));
                        }
                        Err(e) => {
                            tracing::error!("Signal task panicked: {}", e);
                            task_failure = Some(("signal".into(), e.to_string()));
                        }
                    }
                    break;
                }

                // Decoder task finished
                result = &mut decoder_handle => {
                    match result {
                        Ok(Ok(())) => tracing::info!("Decoder task completed"),
                        Ok(Err(e)) => {
                            tracing::error!("Decoder task failed: {}", e);
                            task_failure = Some(("decoder".into(), e.to_string()));
                        }
                        Err(e) => {
                            tracing::error!("Decoder task panicked: {}", e);
                            task_failure = Some(("decoder".into(), e.to_string()));
                        }
                    }
                    break;
                }

                // ── Non-critical tasks ──────────────────────────────────
                // These do NOT break the loop — the data pipeline continues.

                result = &mut ipc_handle, if !ipc_done => {
                    ipc_done = true;
                    match result {
                        Ok(Ok(())) => tracing::info!("IPC task completed"),
                        Ok(Err(e)) => {
                            tracing::warn!("IPC task failed (non-critical): {}", e);
                            let mut state = self.shared_state.write().await;
                            if state.task_error.is_none() {
                                state.task_error = Some((
                                    "ipc".into(),
                                    format!("{} — IPC disabled", e),
                                ));
                            }
                        }
                        Err(e) => {
                            tracing::warn!("IPC task panicked (non-critical): {}", e);
                            let mut state = self.shared_state.write().await;
                            if state.task_error.is_none() {
                                state.task_error = Some(("ipc".into(), format!("panicked: {}", e)));
                            }
                        }
                    }
                    // Continue the loop — data pipeline is unaffected.
                }

                result = &mut action_handle, if !action_done => {
                    action_done = true;
                    match result {
                        Ok(Ok(())) => tracing::info!("Action task completed"),
                        Ok(Err(e)) => {
                            tracing::warn!("Action task failed (non-critical): {}", e);
                            let mut state = self.shared_state.write().await;
                            if state.task_error.is_none() {
                                state.task_error = Some((
                                    "action".into(),
                                    format!("{} — HID output disabled", e),
                                ));
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Action task panicked (non-critical): {}", e);
                            let mut state = self.shared_state.write().await;
                            if state.task_error.is_none() {
                                state.task_error = Some(("action".into(), format!("panicked: {}", e)));
                            }
                        }
                    }
                    // Continue the loop — data pipeline is unaffected.
                }

            }
        }

        // Abort tasks that are still running.
        decoder_handle.abort();
        if !ipc_done {
            ipc_handle.abort();
        }
        if !action_done {
            action_handle.abort();
        }
        if !latency_monitor_done && let Some(handle) = &mut latency_monitor_handle {
            handle.abort();
        }
        if !session_logger_done && let Some(handle) = &mut session_logger_handle {
            handle.abort();
        }
        if !recording_done {
            if let Some(handle) = &mut recording_handle {
                handle.abort();
            }
        }
        if let Some(handle) = &mut outlet_handle {
            handle.abort();
        }

        // Mark service as inactive, store critical failure, and clean up
        // stale connection flags so the GUI doesn't show "Connected" for
        // streams that are no longer active.
        {
            let mut state = self.shared_state.write().await;
            state.active = false;
            // Critical failure overwrites any prior non-critical warning.
            if task_failure.is_some() {
                state.task_error = task_failure;
            }
            state.device_connected = false;
            state.device_name = None;
            state.decoder_ready = false;
            state.decoder_model_version = None;
            state.ipc_connected = false;
            state.ipc_simulated = false;
            state.ml_bridge_connected = false;
            state.ml_bridge_stalled = false;
            state.ml_bridge_last_heartbeat_us = None;
            state.trainer_replay_size = None;
            state.trainer_step = None;
            state.trainer_policy_loss = None;
            state.trainer_value_loss = None;
            state.trainer_entropy = None;
            state.trainer_last_error = None;
            state.candidate_promotions_succeeded = 0;
            state.candidate_promotions_rejected = 0;
            state.candidate_last_outcome = None;
            state.runtime_mode_state = RuntimeModeState::Degraded;
            state.enabled_capabilities.clear();
            state.limited_capabilities_message = None;
            state.last_runtime_mode_alert_us = None;
            state.fallback_model_kind = None;
            state.calibration_mode = false;
            state.decode_latency_last_us = 0;
            state.decode_latency_p95_us = 0;
            state.signal_latency_last_us = 0;
            state.signal_latency_p95_us = 0;
            state.action_latency_last_us = 0;
            state.action_latency_p95_us = 0;
            state.latency_degraded = false;
            state.latency_alert_message = None;
            state.recording_active = false;
            state.current_session_id = None;
            for stream in &mut state.discovered_streams {
                stream.connected = false;
            }
        }

        tracing::info!("Service shutdown complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{IntegrityStage, ServiceState};

    #[test]
    fn integrity_rollup_stays_warning_until_escalation_threshold() {
        let mut state = ServiceState::default();
        state.set_signal_integrity_snapshot(3, 3, 1);

        assert_eq!(state.integrity_issue_count, 3);
        assert!(!state.pipeline_integrity_degraded);
        assert!(
            state
                .stage_health_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("signal:degraded(3)"))
        );
    }

    #[test]
    fn integrity_rollup_degrades_when_all_eeg_streams_are_impacted() {
        let mut state = ServiceState::default();
        state.set_signal_integrity_snapshot(2, 2, 2);

        assert!(state.pipeline_integrity_degraded);
        assert_eq!(state.integrity_issue_count, 2);
        assert!(
            state
                .stage_health_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("all_eeg_impacted(2/2)"))
        );
    }

    #[test]
    fn integrity_rollup_degrades_after_repeated_critical_violations() {
        let mut state = ServiceState::default();
        for _ in 0..ServiceState::INTEGRITY_CRITICAL_ISSUES_THRESHOLD {
            state.register_integrity_issue(IntegrityStage::Ipc, false);
        }

        assert!(state.pipeline_integrity_degraded);
        assert_eq!(
            state.integrity_issue_count,
            ServiceState::INTEGRITY_CRITICAL_ISSUES_THRESHOLD
        );
        assert!(
            state
                .stage_health_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("critical_threshold"))
        );
    }
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
