//! # Runtime Control Protocol Types
//!
//! Serializable request/response contracts for local service control channels
//! (for example Windows named pipes or localhost loopback sockets).

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
}

/// Serializable runtime snapshot for control clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ControlSnapshot {
    pub running: bool,
    pub calibration_mode: bool,
    pub output_enabled: bool,
    pub profile_ready: bool,
    pub decoder_ready: bool,
    pub decoder_model_version: Option<String>,
    pub signal_quality: f32,
    pub signal_latency_last_us: u64,
    pub signal_latency_p95_us: u64,
    pub decode_latency_last_us: u64,
    pub decode_latency_p95_us: u64,
    pub action_latency_last_us: u64,
    pub action_latency_p95_us: u64,
    pub actions_emitted: u64,
    pub errors_detected: u64,
    pub ipc_connected: bool,
    pub device_connected: bool,
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
}

/// Control response variants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlResponsePayload {
    /// Command accepted (no additional payload).
    Ack,
    /// Current runtime status snapshot.
    Snapshot { snapshot: ControlSnapshot },
    /// Command rejected/failed.
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::{
        ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload, ControlSnapshot,
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
            calibration_mode: false,
            output_enabled: true,
            profile_ready: true,
            decoder_ready: true,
            decoder_model_version: Some("v1".to_string()),
            signal_quality: 0.8,
            signal_latency_last_us: 1000,
            signal_latency_p95_us: 1300,
            decode_latency_last_us: 900,
            decode_latency_p95_us: 1200,
            action_latency_last_us: 1400,
            action_latency_p95_us: 2000,
            actions_emitted: 42,
            errors_detected: 1,
            ipc_connected: true,
            device_connected: true,
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
