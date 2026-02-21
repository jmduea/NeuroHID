//! Shared service state accessible by all tasks.

use neurohid_types::{config::FallbackPolicy, control::RuntimeModeState, device::DiscoveredStream};

use super::IntegrityStage;

#[derive(Debug, Clone, Copy)]
pub(crate) struct IntegrityStageMetrics {
    pub(crate) issues: u64,
    pub(crate) degraded: bool,
}

impl IntegrityStageMetrics {
    pub(crate) const fn ok() -> Self {
        Self {
            issues: 0,
            degraded: false,
        }
    }
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

    /// Outlet slot identifier: "built-in" or extension name (for snapshot).
    pub outlet_name: Option<String>,

    /// Signal preprocessing slot identifier: "built-in" or extension name.
    pub signal_name: Option<String>,

    /// Decoder slot identifier: "built-in" or extension name.
    pub decoder_name: Option<String>,

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
    pub(crate) integrity_device: IntegrityStageMetrics,
    pub(crate) integrity_signal: IntegrityStageMetrics,
    pub(crate) integrity_decoder: IntegrityStageMetrics,
    pub(crate) integrity_action: IntegrityStageMetrics,
    pub(crate) integrity_ipc: IntegrityStageMetrics,
    pub(crate) integrity_signal_eeg_streams_total: u64,
    pub(crate) integrity_signal_eeg_streams_degraded: u64,
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
            outlet_name: None,
            signal_name: None,
            decoder_name: None,
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
    pub(crate) const INTEGRITY_CRITICAL_ISSUES_THRESHOLD: u64 = 25;

    pub(crate) fn stage_metrics_mut(
        &mut self,
        stage: IntegrityStage,
    ) -> &mut IntegrityStageMetrics {
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

    pub(crate) fn recompute_integrity_rollup(&mut self) {
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
