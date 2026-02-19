//! Unified IPC envelope and channel contracts.
//!
//! This module defines the complete IPC wire protocol: payload types, envelope
//! shape, logical channels, and runtime event subscriptions. All traffic between
//! the Rust runtime, trainer processes, and observer clients (Hub, notebooks,
//! scripts) is multiplexed over a single endpoint using these types.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    Timestamp,
    action::Action,
    control::{
        ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload, ControlSnapshot,
        TrainerSnapshot,
    },
    event::StreamMarker,
    learning::{CandidateModelMetrics, TrainingEpisode},
    model::ModelManifest,
    now_micros,
    observation::Observation,
    signal::{FeatureVector, Sample},
};

// ---------------------------------------------------------------------------
// Protocol version
// ---------------------------------------------------------------------------

/// Protocol version for unified IPC.
pub const IPC_PROTOCOL_VERSION: u16 = 3;

// ---------------------------------------------------------------------------
// Payload types (shared across trainer stream and runtime events)
// ---------------------------------------------------------------------------

/// Message kinds in the runtime ML protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMlKind {
    Hello,
    SessionBoundary,
    DecisionEvent,
    ErrpWindow,
    RuntimeTelemetry,
    Ping,
    Shutdown,
    ErrpResult,
    TrainerStatus,
    CandidateModelReady,
    Pong,
    Ack,
    Error,
}

/// Peer role in handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMlRole {
    Runtime,
    Trainer,
}

/// `hello` payload exchanged during protocol handshake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: String,
    /// Either `runtime` or `trainer`.
    pub role: RuntimeMlRole,
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_schema_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_schema_version: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decoder_model_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trainer_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trainer_version: Option<String>,
}

/// Session boundary event payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionBoundary {
    pub event: SessionBoundaryEvent,
    pub reason: String,
    pub started_at_us: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBoundaryEvent {
    Start,
    End,
}

/// Runtime decision payload for trainer replay and analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionEvent {
    pub decision_id: String,
    pub timestamp_us: Timestamp,
    pub feature_values: Vec<f32>,
    pub action: Action,
    pub decoder_confidence: f32,
    pub signal_quality: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decoder_model_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
}

/// ErrP analysis window payload sent from runtime to trainer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrpWindow {
    pub decision_id: String,
    pub action_timestamp_us: Timestamp,
    pub window_start_us: Timestamp,
    pub window_end_us: Timestamp,
    pub sample_rate_hz: f32,
    pub channel_labels: Vec<String>,
    pub channel_data: Vec<Vec<f32>>,
    pub signal_quality: f32,
}

/// Runtime latency/queue telemetry payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeTelemetry {
    pub signal_latency_p95_us: u64,
    pub decode_latency_p95_us: u64,
    pub action_latency_p95_us: u64,
    pub decision_queue_depth: usize,
    pub errp_queue_depth: usize,
    pub dropped_ml_messages: u64,
}

/// Ping payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ping {
    pub ping_id: String,
    pub timestamp_us: Timestamp,
}

/// Shutdown payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shutdown {
    pub reason: String,
}

/// ErrP inference result payload sent by trainer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrpResult {
    pub decision_id: String,
    pub action_timestamp_us: Timestamp,
    pub detection_timestamp_us: Timestamp,
    pub error_probability: f32,
    pub classification_confidence: f32,
    /// `good`, `acceptable`, `poor`, or `unusable`.
    pub signal_quality: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_magnitude: Option<f32>,
    pub detection_latency_us: i64,
}

/// Trainer state payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrainerStatus {
    pub state: String,
    pub replay_size: u64,
    pub training_step: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_loss: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_loss: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entropy: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Trainer-produced candidate notification payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateModelReady {
    pub profile_id: String,
    pub artifact_dir: String,
    pub manifest: ModelManifest,
    pub metrics: CandidateModelMetrics,
    pub source_run_id: String,
    pub created_at_us: Timestamp,
}

/// Pong payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pong {
    pub ping_id: String,
    pub timestamp_us: Timestamp,
}

/// Ack payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ack {
    pub ack_kind: RuntimeMlKind,
    pub ack_seq: u64,
}

/// Protocol/application error payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

/// Lightweight projection for session replay logging.
impl From<DecisionEvent> for TrainingEpisode {
    fn from(value: DecisionEvent) -> Self {
        Self {
            timestamp: value.timestamp_us,
            feature_values: value.feature_values,
            action: value.action,
            decoder_confidence: value.decoder_confidence,
            signal_quality: value.signal_quality,
            decoder_model_version: value.decoder_model_version,
            errp_error_probability: None,
            errp_confidence: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Envelope and channels
// ---------------------------------------------------------------------------

/// Logical channels multiplexed over a single IPC endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcChannel {
    #[serde(rename = "control.rpc")]
    ControlRpc,
    #[serde(rename = "trainer.stream")]
    TrainerStream,
    #[serde(rename = "runtime.events")]
    RuntimeEvents,
}

/// Generic IPC envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcEnvelope {
    /// Protocol version (`3`).
    pub v: u16,
    /// Logical channel.
    pub channel: IpcChannel,
    /// Message type tag scoped to `channel`.
    pub msg_type: String,
    /// Monotonic sequence number within sender session.
    pub seq: u64,
    /// Optional correlation id for request-response flows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Optional sender session id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Send timestamp in microseconds since Unix epoch.
    pub sent_at_us: Timestamp,
    /// Typed payload encoded as JSON value.
    pub payload: serde_json::Value,
}

impl IpcEnvelope {
    /// Build an envelope from a strongly typed payload.
    pub fn new<T: Serialize>(
        channel: IpcChannel,
        msg_type: impl Into<String>,
        seq: u64,
        request_id: Option<String>,
        session_id: Option<String>,
        payload: &T,
    ) -> Result<Self, String> {
        let encoded =
            serde_json::to_value(payload).map_err(|e| format!("payload encode failed: {e}"))?;
        Ok(Self {
            v: IPC_PROTOCOL_VERSION,
            channel,
            msg_type: msg_type.into(),
            seq,
            request_id,
            session_id,
            sent_at_us: now_micros(),
            payload: encoded,
        })
    }

    /// Decode payload into a strongly typed body.
    pub fn decode_payload<T: DeserializeOwned>(&self) -> Result<T, String> {
        serde_json::from_value(self.payload.clone())
            .map_err(|e| format!("payload decode failed: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Control RPC
// ---------------------------------------------------------------------------

/// Control RPC request payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlRpcRequest {
    pub request_id: Option<String>,
    pub command: ControlCommand,
}

impl From<ControlRequest> for ControlRpcRequest {
    fn from(value: ControlRequest) -> Self {
        Self {
            request_id: value.request_id,
            command: value.command,
        }
    }
}

impl From<ControlRpcRequest> for ControlRequest {
    fn from(value: ControlRpcRequest) -> Self {
        Self {
            request_id: value.request_id,
            command: value.command,
        }
    }
}

/// Control RPC response payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlRpcResponse {
    pub request_id: Option<String>,
    pub payload: ControlRpcResponsePayload,
}

/// Control RPC response variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "mirrors ControlResponsePayload for IPC wire compatibility"
)]
pub enum ControlRpcResponsePayload {
    Ack,
    Snapshot { snapshot: ControlSnapshot },
    TrainerSnapshot { snapshot: TrainerSnapshot },
    Error { message: String },
}

impl From<ControlResponse> for ControlRpcResponse {
    fn from(value: ControlResponse) -> Self {
        Self {
            request_id: value.request_id,
            payload: value.payload.into(),
        }
    }
}

impl From<ControlRpcResponse> for ControlResponse {
    fn from(value: ControlRpcResponse) -> Self {
        Self {
            request_id: value.request_id,
            payload: value.payload.into(),
        }
    }
}

impl From<ControlResponsePayload> for ControlRpcResponsePayload {
    fn from(value: ControlResponsePayload) -> Self {
        match value {
            ControlResponsePayload::Ack => Self::Ack,
            ControlResponsePayload::Snapshot { snapshot } => Self::Snapshot { snapshot },
            ControlResponsePayload::TrainerSnapshot { snapshot } => {
                Self::TrainerSnapshot { snapshot }
            }
            ControlResponsePayload::Error { message } => Self::Error { message },
        }
    }
}

impl From<ControlRpcResponsePayload> for ControlResponsePayload {
    fn from(value: ControlRpcResponsePayload) -> Self {
        match value {
            ControlRpcResponsePayload::Ack => Self::Ack,
            ControlRpcResponsePayload::Snapshot { snapshot } => Self::Snapshot { snapshot },
            ControlRpcResponsePayload::TrainerSnapshot { snapshot } => {
                Self::TrainerSnapshot { snapshot }
            }
            ControlRpcResponsePayload::Error { message } => Self::Error { message },
        }
    }
}

// ---------------------------------------------------------------------------
// Trainer stream
// ---------------------------------------------------------------------------

/// Trainer stream message kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainerStreamKind {
    Hello,
    SessionBoundary,
    DecisionEvent,
    ErrpWindow,
    RuntimeTelemetry,
    Ping,
    Shutdown,
    ErrpResult,
    TrainerStatus,
    CandidateModelReady,
    Pong,
    Ack,
    Error,
}

impl TrainerStreamKind {
    /// Canonical message type name used in `IpcEnvelope.msg_type`.
    pub const fn as_msg_type(self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::SessionBoundary => "session_boundary",
            Self::DecisionEvent => "decision_event",
            Self::ErrpWindow => "errp_window",
            Self::RuntimeTelemetry => "runtime_telemetry",
            Self::Ping => "ping",
            Self::Shutdown => "shutdown",
            Self::ErrpResult => "errp_result",
            Self::TrainerStatus => "trainer_status",
            Self::CandidateModelReady => "candidate_model_ready",
            Self::Pong => "pong",
            Self::Ack => "ack",
            Self::Error => "error",
        }
    }

    /// Parse trainer message type string.
    pub fn from_msg_type(value: &str) -> Option<Self> {
        match value {
            "hello" => Some(Self::Hello),
            "session_boundary" => Some(Self::SessionBoundary),
            "decision_event" => Some(Self::DecisionEvent),
            "errp_window" => Some(Self::ErrpWindow),
            "runtime_telemetry" => Some(Self::RuntimeTelemetry),
            "ping" => Some(Self::Ping),
            "shutdown" => Some(Self::Shutdown),
            "errp_result" => Some(Self::ErrpResult),
            "trainer_status" => Some(Self::TrainerStatus),
            "candidate_model_ready" => Some(Self::CandidateModelReady),
            "pong" => Some(Self::Pong),
            "ack" => Some(Self::Ack),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl From<RuntimeMlKind> for TrainerStreamKind {
    fn from(value: RuntimeMlKind) -> Self {
        match value {
            RuntimeMlKind::Hello => Self::Hello,
            RuntimeMlKind::SessionBoundary => Self::SessionBoundary,
            RuntimeMlKind::DecisionEvent => Self::DecisionEvent,
            RuntimeMlKind::ErrpWindow => Self::ErrpWindow,
            RuntimeMlKind::RuntimeTelemetry => Self::RuntimeTelemetry,
            RuntimeMlKind::Ping => Self::Ping,
            RuntimeMlKind::Shutdown => Self::Shutdown,
            RuntimeMlKind::ErrpResult => Self::ErrpResult,
            RuntimeMlKind::TrainerStatus => Self::TrainerStatus,
            RuntimeMlKind::CandidateModelReady => Self::CandidateModelReady,
            RuntimeMlKind::Pong => Self::Pong,
            RuntimeMlKind::Ack => Self::Ack,
            RuntimeMlKind::Error => Self::Error,
        }
    }
}

/// Payload wrapper for trainer stream events that keeps explicit typing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrainerStreamPayload {
    Hello { payload: Hello },
    SessionBoundary { payload: SessionBoundary },
    DecisionEvent { payload: DecisionEvent },
    ErrpWindow { payload: ErrpWindow },
    RuntimeTelemetry { payload: RuntimeTelemetry },
    Ping { payload: Ping },
    Shutdown { payload: Shutdown },
    ErrpResult { payload: ErrpResult },
    TrainerStatus { payload: TrainerStatus },
    CandidateModelReady { payload: CandidateModelReady },
    Pong { payload: Pong },
    Ack { payload: Ack },
    Error { payload: ProtocolError },
}

// ---------------------------------------------------------------------------
// Runtime events
// ---------------------------------------------------------------------------

/// Runtime events broadcast to observers (Hub/notebooks/scripts).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[expect(
    clippy::large_enum_variant,
    reason = "IPC payload ergonomics favor one tagged enum for serde wire compatibility"
)]
pub enum RuntimeEvent {
    Snapshot {
        snapshot: ControlSnapshot,
    },
    TrainerSnapshot {
        snapshot: TrainerSnapshot,
    },
    TrainerStatus {
        status: TrainerStatus,
    },
    RuntimeTelemetry {
        telemetry: RuntimeTelemetry,
    },
    Sample {
        sample: Sample,
    },
    FeatureFrame {
        feature: FeatureVector,
    },
    ActionEmitted {
        action: Action,
    },
    Marker {
        marker: StreamMarker,
    },
    ObservationFrame {
        observation: Observation,
    },
    DecisionEvent {
        event: DecisionEvent,
    },
    ErrpWindow {
        window: ErrpWindow,
    },
    ErrpResult {
        result: ErrpResult,
    },
    IntegrityIssue {
        issue: String,
        details: String,
    },
    Lifecycle {
        state: String,
        detail: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        requested_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replay_window_start_seq: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        replay_window_end_seq: Option<u64>,
    },
    BackpressureDrop {
        channel: IpcChannel,
        dropped: u64,
        reason: String,
    },
    Capabilities {
        observation_schema_version: u16,
        channels: Vec<IpcChannel>,
        components: Vec<RuntimeComponentCapability>,
    },
}

/// Runtime event subscription payload (`runtime.events` + `subscribe`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeEventsSubscribe {
    pub families: Vec<String>,
    pub include_snapshot: bool,
    pub include_capabilities: bool,
    pub max_events: Option<u64>,
    pub max_duration_ms: Option<u64>,
    pub resume_from_seq: Option<u64>,
    pub sample_every: u64,
    pub snapshot_interval_ms: u64,
}

impl Default for RuntimeEventsSubscribe {
    fn default() -> Self {
        Self {
            families: Vec::new(),
            include_snapshot: true,
            include_capabilities: true,
            max_events: None,
            max_duration_ms: None,
            resume_from_seq: None,
            sample_every: 1,
            snapshot_interval_ms: 1_000,
        }
    }
}

/// Advertised availability status for one runtime.events component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeComponentCapability {
    pub name: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{
        ControlCommand, ControlRequest, ControlResponse, ControlSnapshot, RuntimeModeState,
    };

    fn sample_snapshot() -> ControlSnapshot {
        ControlSnapshot {
            running: true,
            uptime_secs: 1,
            calibration_mode: false,
            output_enabled: true,
            profile_ready: true,
            decoder_ready: true,
            decoder_model_version: None,
            active_profile_name: None,
            device_name: None,
            device_battery: None,
            signal_quality: 1.0,
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
        }
    }

    #[test]
    fn decision_event_roundtrips_payload() {
        let mut action = crate::Action::none();
        action.confidence = 0.9;
        action.decision_id = Some("dec_1".to_string());
        let payload = DecisionEvent {
            decision_id: "dec_1".to_string(),
            timestamp_us: 123,
            feature_values: vec![0.1, 0.2, 0.3],
            action,
            decoder_confidence: 0.9,
            signal_quality: 0.8,
            decoder_model_version: Some("1.0.0".to_string()),
            stream_id: Some("mock_stream".to_string()),
        };

        let envelope = IpcEnvelope::new(
            IpcChannel::TrainerStream,
            TrainerStreamKind::DecisionEvent.as_msg_type(),
            11,
            None,
            Some("s1".to_string()),
            &payload,
        )
        .expect("build");
        let decoded: DecisionEvent = envelope.decode_payload().expect("decode payload");
        assert_eq!(decoded.decision_id, "dec_1");
        assert_eq!(decoded.action.decision_id.as_deref(), Some("dec_1"));
    }

    #[test]
    fn control_request_roundtrip_via_envelope() {
        let request = ControlRpcRequest::from(ControlRequest {
            request_id: Some("req-1".to_string()),
            command: ControlCommand::Snapshot,
        });
        let envelope = IpcEnvelope::new(
            IpcChannel::ControlRpc,
            "request",
            1,
            request.request_id.clone(),
            None,
            &request,
        )
        .expect("envelope encoding should succeed");
        assert_eq!(envelope.v, IPC_PROTOCOL_VERSION);

        let decoded: ControlRpcRequest = envelope
            .decode_payload()
            .expect("payload decode should succeed");
        assert_eq!(decoded, request);
    }

    #[test]
    fn control_response_mapping_keeps_shape() {
        let response = ControlResponse::snapshot(Some("x".to_string()), sample_snapshot());
        let v3 = ControlRpcResponse::from(response.clone());
        let roundtrip = ControlResponse::from(v3);
        assert_eq!(roundtrip, response);
    }

    #[test]
    fn trainer_kind_from_runtime_ml_kind() {
        let kind = TrainerStreamKind::from(RuntimeMlKind::TrainerStatus);
        assert_eq!(kind, TrainerStreamKind::TrainerStatus);
    }

    #[test]
    fn runtime_event_snapshot_serializes() {
        let event = RuntimeEvent::Snapshot {
            snapshot: sample_snapshot(),
        };
        let encoded = serde_json::to_string(&event).expect("event json should encode");
        assert!(encoded.contains("\"type\":\"snapshot\""));
    }

    #[test]
    fn all_runtime_ml_kinds_have_msg_type() {
        let kinds = [
            RuntimeMlKind::Hello,
            RuntimeMlKind::SessionBoundary,
            RuntimeMlKind::DecisionEvent,
            RuntimeMlKind::ErrpWindow,
            RuntimeMlKind::RuntimeTelemetry,
            RuntimeMlKind::Ping,
            RuntimeMlKind::Shutdown,
            RuntimeMlKind::ErrpResult,
            RuntimeMlKind::TrainerStatus,
            RuntimeMlKind::CandidateModelReady,
            RuntimeMlKind::Pong,
            RuntimeMlKind::Ack,
            RuntimeMlKind::Error,
        ];
        for kind in kinds {
            let stream_kind = TrainerStreamKind::from(kind);
            let msg_type = stream_kind.as_msg_type();
            assert!(
                TrainerStreamKind::from_msg_type(msg_type).is_some(),
                "msg_type round-trip failed for {msg_type}"
            );
        }
    }
}
