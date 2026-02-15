//! Runtime ML protocol v2 message contract.
//!
//! This protocol is used between the Rust runtime and an external trainer
//! process. Messages use a shared envelope with a typed payload serialized
//! as JSON.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    Timestamp,
    action::Action,
    learning::{CandidateModelMetrics, TrainingEpisode},
    model::ModelManifest,
    now_micros,
};

/// Fixed protocol version for runtime ML v2 envelopes.
pub const RUNTIME_ML_PROTOCOL_V2: u16 = 2;

/// Message kinds in the runtime ML v2 protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMlKindV2 {
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

/// Shared runtime ML v2 transport envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeMlEnvelopeV2 {
    /// Protocol version. Must be `2`.
    pub v: u16,
    /// Message kind.
    pub kind: RuntimeMlKindV2,
    /// Monotonic sequence number for this connection/session.
    pub seq: u64,
    /// Send timestamp in microseconds since unix epoch.
    pub sent_at_us: Timestamp,
    /// Runtime session identifier.
    pub session_id: String,
    /// Kind-specific payload.
    pub payload: serde_json::Value,
}

impl RuntimeMlEnvelopeV2 {
    /// Build an envelope from a strongly typed payload.
    pub fn new<T: Serialize>(
        kind: RuntimeMlKindV2,
        seq: u64,
        session_id: impl Into<String>,
        payload: &T,
    ) -> Result<Self, String> {
        let payload =
            serde_json::to_value(payload).map_err(|e| format!("payload encode failed: {e}"))?;
        Ok(Self {
            v: RUNTIME_ML_PROTOCOL_V2,
            kind,
            seq,
            sent_at_us: now_micros(),
            session_id: session_id.into(),
            payload,
        })
    }

    /// Decode the payload into a strongly typed message body.
    pub fn decode_payload<T: DeserializeOwned>(&self) -> Result<T, String> {
        serde_json::from_value(self.payload.clone())
            .map_err(|e| format!("payload decode failed: {e}"))
    }
}

/// `hello` payload exchanged during protocol handshake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelloV2 {
    pub protocol: String,
    /// Either `runtime` or `trainer`.
    pub role: RuntimeMlRoleV2,
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

/// Peer role in handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMlRoleV2 {
    Runtime,
    Trainer,
}

/// Session boundary event payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionBoundaryV2 {
    pub event: SessionBoundaryEventV2,
    pub reason: String,
    pub started_at_us: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionBoundaryEventV2 {
    Start,
    End,
}

/// Runtime decision payload for trainer replay and analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionEventV2 {
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
pub struct ErrpWindowV2 {
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
pub struct RuntimeTelemetryV2 {
    pub signal_latency_p95_us: u64,
    pub decode_latency_p95_us: u64,
    pub action_latency_p95_us: u64,
    pub decision_queue_depth: usize,
    pub errp_queue_depth: usize,
    pub dropped_ml_messages: u64,
}

/// Ping payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PingV2 {
    pub ping_id: String,
    pub timestamp_us: Timestamp,
}

/// Shutdown payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShutdownV2 {
    pub reason: String,
}

/// ErrP inference result payload sent by trainer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrpResultV2 {
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
pub struct TrainerStatusV2 {
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
pub struct CandidateModelReadyV2 {
    pub profile_id: String,
    pub artifact_dir: String,
    pub manifest: ModelManifest,
    pub metrics: CandidateModelMetrics,
    pub source_run_id: String,
    pub created_at_us: Timestamp,
}

/// Pong payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PongV2 {
    pub ping_id: String,
    pub timestamp_us: Timestamp,
}

/// Ack payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AckV2 {
    pub ack_kind: RuntimeMlKindV2,
    pub ack_seq: u64,
}

/// Protocol/application error payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolErrorV2 {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

/// Lightweight projection for session replay logging.
impl From<DecisionEventV2> for TrainingEpisode {
    fn from(value: DecisionEventV2) -> Self {
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

#[cfg(test)]
mod tests {
    use serde_json::to_string;

    use super::{
        DecisionEventV2, HelloV2, RUNTIME_ML_PROTOCOL_V2, RuntimeMlEnvelopeV2, RuntimeMlKindV2,
        RuntimeMlRoleV2,
    };

    #[test]
    fn envelope_roundtrips_json() {
        let payload = HelloV2 {
            protocol: "neurohid_runtime_ml_v2".to_string(),
            role: RuntimeMlRoleV2::Runtime,
            capabilities: vec!["errp_window_stream".to_string()],
            profile_id: Some("p1".to_string()),
            feature_schema_version: Some(1),
            action_schema_version: Some(1),
            decoder_model_version: Some("1.0.0".to_string()),
            trainer_name: None,
            trainer_version: None,
        };

        let envelope =
            RuntimeMlEnvelopeV2::new(RuntimeMlKindV2::Hello, 7, "s1", &payload).expect("build");
        assert_eq!(envelope.v, RUNTIME_ML_PROTOCOL_V2);

        let encoded = serde_json::to_string(&envelope).expect("serialize");
        let decoded: RuntimeMlEnvelopeV2 = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded.kind, RuntimeMlKindV2::Hello);
        let hello: HelloV2 = decoded.decode_payload().expect("decode payload");
        assert_eq!(hello.profile_id.as_deref(), Some("p1"));
    }

    #[test]
    fn decision_event_roundtrips_payload() {
        let mut action = crate::Action::none();
        action.confidence = 0.9;
        action.decision_id = Some("dec_1".to_string());
        let payload = DecisionEventV2 {
            decision_id: "dec_1".to_string(),
            timestamp_us: 123,
            feature_values: vec![0.1, 0.2, 0.3],
            action,
            decoder_confidence: 0.9,
            signal_quality: 0.8,
            decoder_model_version: Some("1.0.0".to_string()),
            stream_id: Some("mock_stream".to_string()),
        };

        let envelope = RuntimeMlEnvelopeV2::new(RuntimeMlKindV2::DecisionEvent, 11, "s1", &payload)
            .expect("build");
        let decoded: DecisionEventV2 = envelope.decode_payload().expect("decode payload");
        assert_eq!(decoded.decision_id, "dec_1");
        assert_eq!(decoded.action.decision_id.as_deref(), Some("dec_1"));
    }

    #[test]
    fn runtime_ml_protocol_docs_cover_current_contract() {
        let doc = include_str!("../../../docs/runtime-ml-protocol-v2.md").to_lowercase();
        assert!(doc.contains("\"v\": 2"));

        let message_kinds = [
            RuntimeMlKindV2::Hello,
            RuntimeMlKindV2::SessionBoundary,
            RuntimeMlKindV2::DecisionEvent,
            RuntimeMlKindV2::ErrpWindow,
            RuntimeMlKindV2::RuntimeTelemetry,
            RuntimeMlKindV2::Ping,
            RuntimeMlKindV2::Shutdown,
            RuntimeMlKindV2::ErrpResult,
            RuntimeMlKindV2::TrainerStatus,
            RuntimeMlKindV2::CandidateModelReady,
            RuntimeMlKindV2::Pong,
            RuntimeMlKindV2::Ack,
            RuntimeMlKindV2::Error,
        ];

        for kind in message_kinds {
            let token = to_string(&kind)
                .expect("serialize kind")
                .trim_matches('"')
                .to_string();
            assert!(
                doc.contains(&token),
                "protocol doc is missing message kind token: {token}"
            );
        }

        assert_eq!(RUNTIME_ML_PROTOCOL_V2, 2);
    }
}
