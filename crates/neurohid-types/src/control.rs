//! # Runtime Control Protocol Types
//!
//! Serializable request/response contracts for local service control channels
//! (for example Windows named pipes or localhost loopback sockets).

use crate::{
    config::{FallbackPolicy, SignalConfig},
    device::DiscoveredStream,
};
use serde::{Deserialize, Serialize};

/// Request sent from control clients to a running service instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlRequest {
    /// Optional correlation identifier.
    pub request_id: Option<String>,
    /// Requested service action.
    pub command: ControlCommand,
}

impl ControlRequest {
    /// Build a request without a correlation id.
    pub fn new(command: ControlCommand) -> Self {
        Self {
            request_id: None,
            command,
        }
    }
}

/// Supported runtime control commands.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlCommand {
    /// Fetch current runtime snapshot.
    Snapshot,
    /// Request graceful runtime shutdown.
    Shutdown,
    /// Toggle calibration mode.
    SetCalibrationMode { enabled: bool },
    /// Pause or resume HID output.
    SetOutputEnabled { enabled: bool },
    /// Reload active model artifacts.
    ReloadModel,
    /// Promote candidate model artifacts with guardrail checks.
    PromoteCandidateModel,
    /// Trigger stream discovery refresh.
    RescanStreams,
    /// Connect to one discovered stream.
    ConnectStream { stream_id: String },
    /// Disconnect one discovered stream.
    DisconnectStream { stream_id: String },
    /// Toggle online learning state.
    SetLearningEnabled { enabled: bool },
    /// Force a reconnect attempt for the runtime ML bridge.
    MlBridgeReconnect,
    /// Fetch trainer-side status snapshot cached by runtime.
    TrainerSnapshot,
    /// Replace runtime fallback policy.
    SetFallbackPolicy { policy: FallbackPolicy },
    /// Replace runtime signal configuration.
    SetSignalConfig { signal: SignalConfig },
    /// Start session recording; optional output path overrides config default.
    StartRecording {
        output_path: Option<std::path::PathBuf>,
    },
    /// Stop current session recording.
    StopRecording,
}

/// Runtime mode classification derived from model/bridge health.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeModeState {
    Full,
    Fallback,
    Degraded,
}

/// Trainer status snapshot exported over the control channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrainerSnapshot {
    pub trainer_connected: bool,
    pub trainer_state: String,
    pub replay_size: u64,
    pub training_step: u64,
    pub last_heartbeat_us: Option<i64>,
    pub last_error: Option<String>,
    pub protocol_version: Option<u16>,
}

/// Serializable runtime snapshot for control clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlSnapshot {
    pub running: bool,
    pub uptime_secs: u64,
    pub calibration_mode: bool,
    pub output_enabled: bool,
    pub profile_ready: bool,
    pub decoder_ready: bool,
    pub decoder_model_version: Option<String>,
    pub active_profile_name: Option<String>,
    pub device_name: Option<String>,
    /// Outlet slot identifier: "built-in" or extension name.
    #[serde(default)]
    pub outlet_name: Option<String>,
    /// Signal preprocessing slot identifier: "built-in" or extension name.
    #[serde(default)]
    pub signal_name: Option<String>,
    /// Decoder slot identifier: "built-in" or extension name.
    #[serde(default)]
    pub decoder_name: Option<String>,
    pub device_battery: Option<u8>,
    pub signal_quality: f32,
    pub signal_latency_last_us: u64,
    pub signal_latency_p95_us: u64,
    pub decode_latency_last_us: u64,
    pub decode_latency_p95_us: u64,
    pub action_latency_last_us: u64,
    pub action_latency_p95_us: u64,
    pub latency_degraded: bool,
    pub latency_alert_message: Option<String>,
    pub actions_emitted: u64,
    pub errors_detected: u64,
    pub ipc_connected: bool,
    pub ipc_simulated: bool,
    pub learning_enabled: bool,
    pub ml_bridge_connected: bool,
    pub ml_bridge_stalled: bool,
    pub runtime_mode_state: RuntimeModeState,
    pub enabled_capabilities: Vec<String>,
    pub limited_capabilities_message: Option<String>,
    pub fallback_model_kind: Option<String>,
    pub trainer_replay_size: Option<u64>,
    pub trainer_step: Option<u64>,
    pub trainer_policy_loss: Option<f32>,
    pub trainer_value_loss: Option<f32>,
    pub trainer_entropy: Option<f32>,
    pub trainer_last_error: Option<String>,
    pub candidate_promotions_succeeded: u64,
    pub candidate_promotions_rejected: u64,
    pub candidate_last_outcome: Option<String>,
    pub ml_protocol_version: Option<u16>,
    pub device_connected: bool,
    pub task_error: Option<(String, String)>,
    pub discovered_streams: Vec<DiscoveredStream>,
    #[serde(default)]
    pub routed_eeg_streams: u64,
    #[serde(default)]
    pub routed_motion_streams: u64,
    #[serde(default)]
    pub routed_auxiliary_streams: u64,
    #[serde(default)]
    pub routed_unknown_streams: u64,
    #[serde(default)]
    pub pipeline_integrity_degraded: bool,
    #[serde(default)]
    pub integrity_issue_count: u64,
    #[serde(default)]
    pub stage_health_summary: Option<String>,
    /// Whether a session recording is currently active.
    #[serde(default)]
    pub recording_active: bool,
    /// Session id of the current recording, if any.
    #[serde(default)]
    pub current_session_id: Option<String>,
}

impl Default for ControlSnapshot {
    fn default() -> Self {
        Self {
            running: false,
            uptime_secs: 0,
            calibration_mode: false,
            output_enabled: true,
            profile_ready: false,
            decoder_ready: false,
            decoder_model_version: None,
            active_profile_name: None,
            device_name: None,
            outlet_name: None,
            signal_name: None,
            decoder_name: None,
            device_battery: None,
            signal_quality: 0.0,
            signal_latency_last_us: 0,
            signal_latency_p95_us: 0,
            decode_latency_last_us: 0,
            decode_latency_p95_us: 0,
            action_latency_last_us: 0,
            action_latency_p95_us: 0,
            latency_degraded: false,
            latency_alert_message: None,
            actions_emitted: 0,
            errors_detected: 0,
            ipc_connected: false,
            ipc_simulated: false,
            learning_enabled: true,
            ml_bridge_connected: false,
            ml_bridge_stalled: false,
            runtime_mode_state: RuntimeModeState::Degraded,
            enabled_capabilities: Vec::new(),
            limited_capabilities_message: None,
            fallback_model_kind: None,
            trainer_replay_size: None,
            trainer_step: None,
            trainer_policy_loss: None,
            trainer_value_loss: None,
            trainer_entropy: None,
            trainer_last_error: None,
            candidate_promotions_succeeded: 0,
            candidate_promotions_rejected: 0,
            candidate_last_outcome: None,
            ml_protocol_version: None,
            device_connected: false,
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
        }
    }
}

/// Response emitted by control servers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlResponse {
    /// Echoed request id when provided by caller.
    pub request_id: Option<String>,
    /// Response payload.
    pub payload: ControlResponsePayload,
}

impl ControlResponse {
    pub fn ack(request_id: Option<String>) -> Self {
        Self {
            request_id,
            payload: ControlResponsePayload::Ack,
        }
    }

    pub fn snapshot(request_id: Option<String>, snapshot: ControlSnapshot) -> Self {
        Self {
            request_id,
            payload: ControlResponsePayload::Snapshot { snapshot },
        }
    }

    pub fn error(request_id: Option<String>, message: String) -> Self {
        Self {
            request_id,
            payload: ControlResponsePayload::Error { message },
        }
    }

    pub fn trainer_snapshot(request_id: Option<String>, snapshot: TrainerSnapshot) -> Self {
        Self {
            request_id,
            payload: ControlResponsePayload::TrainerSnapshot { snapshot },
        }
    }

    pub fn recording_started(
        request_id: Option<String>,
        session_id: String,
        output_path: String,
    ) -> Self {
        Self {
            request_id,
            payload: ControlResponsePayload::RecordingStarted {
                session_id,
                output_path,
            },
        }
    }

    pub fn recording_stopped(request_id: Option<String>, session_id: String) -> Self {
        Self {
            request_id,
            payload: ControlResponsePayload::RecordingStopped { session_id },
        }
    }
}

/// Control response variants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "IPC payload ergonomics favor one tagged enum for serde wire compatibility"
)]
pub enum ControlResponsePayload {
    /// Command accepted (no additional payload).
    Ack,
    /// Current runtime status snapshot.
    Snapshot { snapshot: ControlSnapshot },
    /// Runtime-cached trainer status.
    TrainerSnapshot { snapshot: TrainerSnapshot },
    /// Command rejected/failed.
    Error { message: String },
    /// Recording started; includes session id and output path.
    RecordingStarted {
        session_id: String,
        output_path: String,
    },
    /// Recording stopped; includes session id.
    RecordingStopped { session_id: String },
}

#[cfg(test)]
mod tests {
    use super::{
        ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload, ControlSnapshot,
        RuntimeModeState,
    };

    #[test]
    fn control_request_roundtrips_json() {
        let request = ControlRequest {
            request_id: Some("abc-123".to_string()),
            command: ControlCommand::SetOutputEnabled { enabled: false },
        };

        let json = serde_json::to_string(&request).expect("request should serialize");
        let decoded: ControlRequest =
            serde_json::from_str(&json).expect("request should deserialize");
        assert_eq!(decoded, request);
    }

    #[test]
    fn snapshot_response_roundtrips_json() {
        let snapshot = ControlSnapshot {
            running: true,
            uptime_secs: 123,
            calibration_mode: false,
            output_enabled: true,
            profile_ready: true,
            decoder_ready: true,
            decoder_model_version: Some("v1".to_string()),
            active_profile_name: Some("default".to_string()),
            device_name: Some("Mock EEG".to_string()),
            outlet_name: None,
            signal_name: None,
            decoder_name: None,
            device_battery: Some(100),
            signal_quality: 0.8,
            signal_latency_last_us: 1000,
            signal_latency_p95_us: 1300,
            decode_latency_last_us: 900,
            decode_latency_p95_us: 1200,
            action_latency_last_us: 1400,
            action_latency_p95_us: 2000,
            latency_degraded: false,
            latency_alert_message: None,
            actions_emitted: 42,
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
            trainer_replay_size: Some(100),
            trainer_step: Some(12),
            trainer_policy_loss: Some(0.12),
            trainer_value_loss: Some(0.34),
            trainer_entropy: Some(0.08),
            trainer_last_error: None,
            candidate_promotions_succeeded: 3,
            candidate_promotions_rejected: 1,
            candidate_last_outcome: Some("promoted candidate model".to_string()),
            ml_protocol_version: Some(2),
            device_connected: true,
            task_error: None,
            discovered_streams: vec![],
            routed_eeg_streams: 1,
            routed_motion_streams: 0,
            routed_auxiliary_streams: 0,
            routed_unknown_streams: 0,
            pipeline_integrity_degraded: false,
            integrity_issue_count: 0,
            stage_health_summary: Some("signal:ok".to_string()),
            recording_active: false,
            current_session_id: None,
        };

        let response = ControlResponse::snapshot(Some("id-1".to_string()), snapshot.clone());
        let json = serde_json::to_string(&response).expect("response should serialize");
        let decoded: ControlResponse =
            serde_json::from_str(&json).expect("response should deserialize");
        assert_eq!(decoded, response);
        assert_eq!(
            decoded.payload,
            ControlResponsePayload::Snapshot { snapshot }
        );
    }
}
