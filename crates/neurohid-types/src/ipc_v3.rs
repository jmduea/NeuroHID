//! Unified IPC v3 envelope and channel contracts.
//!
//! IPC v3 standardizes control RPC, trainer stream traffic, and runtime event
//! subscriptions under one common envelope shape.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    Timestamp,
    action::Action,
    control::{
        ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload, ControlSnapshot,
        TrainerSnapshot,
    },
    event::StreamMarker,
    ipc_v2::{
        AckV2, CandidateModelReadyV2, DecisionEventV2, ErrpResultV2, ErrpWindowV2, HelloV2, PingV2,
        PongV2, ProtocolErrorV2, RuntimeMlKindV2, RuntimeTelemetryV2, SessionBoundaryV2,
        ShutdownV2, TrainerStatusV2,
    },
    now_micros,
    observation::Observation,
    signal::{FeatureVector, Sample},
};

/// Protocol version for unified IPC.
pub const IPC_PROTOCOL_V3: u16 = 3;

/// Logical channels multiplexed over a single IPC endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpcChannelV3 {
    #[serde(rename = "control.rpc")]
    ControlRpc,
    #[serde(rename = "trainer.stream")]
    TrainerStream,
    #[serde(rename = "runtime.events")]
    RuntimeEvents,
}

/// Generic IPC v3 envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcEnvelopeV3 {
    /// Protocol version (`3`).
    pub v: u16,
    /// Logical channel.
    pub channel: IpcChannelV3,
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

impl IpcEnvelopeV3 {
    /// Build an envelope from a strongly typed payload.
    pub fn new<T: Serialize>(
        channel: IpcChannelV3,
        msg_type: impl Into<String>,
        seq: u64,
        request_id: Option<String>,
        session_id: Option<String>,
        payload: &T,
    ) -> Result<Self, String> {
        let encoded =
            serde_json::to_value(payload).map_err(|e| format!("payload encode failed: {e}"))?;
        Ok(Self {
            v: IPC_PROTOCOL_V3,
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

/// Control RPC request payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlRpcRequestV3 {
    pub request_id: Option<String>,
    pub command: ControlCommand,
}

impl From<ControlRequest> for ControlRpcRequestV3 {
    fn from(value: ControlRequest) -> Self {
        Self {
            request_id: value.request_id,
            command: value.command,
        }
    }
}

impl From<ControlRpcRequestV3> for ControlRequest {
    fn from(value: ControlRpcRequestV3) -> Self {
        Self {
            request_id: value.request_id,
            command: value.command,
        }
    }
}

/// Control RPC response payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlRpcResponseV3 {
    pub request_id: Option<String>,
    pub payload: ControlRpcResponsePayloadV3,
}

/// Control RPC response variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum ControlRpcResponsePayloadV3 {
    Ack,
    Snapshot { snapshot: ControlSnapshot },
    TrainerSnapshot { snapshot: TrainerSnapshot },
    Error { message: String },
}

impl From<ControlResponse> for ControlRpcResponseV3 {
    fn from(value: ControlResponse) -> Self {
        Self {
            request_id: value.request_id,
            payload: value.payload.into(),
        }
    }
}

impl From<ControlRpcResponseV3> for ControlResponse {
    fn from(value: ControlRpcResponseV3) -> Self {
        Self {
            request_id: value.request_id,
            payload: value.payload.into(),
        }
    }
}

impl From<ControlResponsePayload> for ControlRpcResponsePayloadV3 {
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

impl From<ControlRpcResponsePayloadV3> for ControlResponsePayload {
    fn from(value: ControlRpcResponsePayloadV3) -> Self {
        match value {
            ControlRpcResponsePayloadV3::Ack => Self::Ack,
            ControlRpcResponsePayloadV3::Snapshot { snapshot } => Self::Snapshot { snapshot },
            ControlRpcResponsePayloadV3::TrainerSnapshot { snapshot } => {
                Self::TrainerSnapshot { snapshot }
            }
            ControlRpcResponsePayloadV3::Error { message } => Self::Error { message },
        }
    }
}

/// Trainer stream message kinds for IPC v3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainerStreamKindV3 {
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

impl TrainerStreamKindV3 {
    /// Canonical message type name used in `IpcEnvelopeV3.msg_type`.
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

impl From<RuntimeMlKindV2> for TrainerStreamKindV3 {
    fn from(value: RuntimeMlKindV2) -> Self {
        match value {
            RuntimeMlKindV2::Hello => Self::Hello,
            RuntimeMlKindV2::SessionBoundary => Self::SessionBoundary,
            RuntimeMlKindV2::DecisionEvent => Self::DecisionEvent,
            RuntimeMlKindV2::ErrpWindow => Self::ErrpWindow,
            RuntimeMlKindV2::RuntimeTelemetry => Self::RuntimeTelemetry,
            RuntimeMlKindV2::Ping => Self::Ping,
            RuntimeMlKindV2::Shutdown => Self::Shutdown,
            RuntimeMlKindV2::ErrpResult => Self::ErrpResult,
            RuntimeMlKindV2::TrainerStatus => Self::TrainerStatus,
            RuntimeMlKindV2::CandidateModelReady => Self::CandidateModelReady,
            RuntimeMlKindV2::Pong => Self::Pong,
            RuntimeMlKindV2::Ack => Self::Ack,
            RuntimeMlKindV2::Error => Self::Error,
        }
    }
}

/// Runtime events broadcast to observers (Hub/notebooks/scripts).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeEventV3 {
    Snapshot {
        snapshot: ControlSnapshot,
    },
    TrainerSnapshot {
        snapshot: TrainerSnapshot,
    },
    TrainerStatus {
        status: TrainerStatusV2,
    },
    RuntimeTelemetry {
        telemetry: RuntimeTelemetryV2,
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
        event: DecisionEventV2,
    },
    ErrpWindow {
        window: ErrpWindowV2,
    },
    ErrpResult {
        result: ErrpResultV2,
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
        channel: IpcChannelV3,
        dropped: u64,
        reason: String,
    },
    Capabilities {
        observation_schema_version: u16,
        channels: Vec<IpcChannelV3>,
        components: Vec<RuntimeComponentCapabilityV3>,
    },
}

/// Runtime event subscription payload (`runtime.events` + `subscribe`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimeEventsSubscribeV3 {
    pub families: Vec<String>,
    pub include_snapshot: bool,
    pub include_capabilities: bool,
    pub max_events: Option<u64>,
    pub max_duration_ms: Option<u64>,
    pub resume_from_seq: Option<u64>,
    pub sample_every: u64,
    pub snapshot_interval_ms: u64,
}

impl Default for RuntimeEventsSubscribeV3 {
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
pub struct RuntimeComponentCapabilityV3 {
    pub name: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}

/// Payload wrapper for trainer stream events that keeps explicit typing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrainerStreamPayloadV3 {
    Hello { payload: HelloV2 },
    SessionBoundary { payload: SessionBoundaryV2 },
    DecisionEvent { payload: DecisionEventV2 },
    ErrpWindow { payload: ErrpWindowV2 },
    RuntimeTelemetry { payload: RuntimeTelemetryV2 },
    Ping { payload: PingV2 },
    Shutdown { payload: ShutdownV2 },
    ErrpResult { payload: ErrpResultV2 },
    TrainerStatus { payload: TrainerStatusV2 },
    CandidateModelReady { payload: CandidateModelReadyV2 },
    Pong { payload: PongV2 },
    Ack { payload: AckV2 },
    Error { payload: ProtocolErrorV2 },
}

#[cfg(test)]
mod tests {
    use super::{
        ControlRpcRequestV3, ControlRpcResponseV3, IPC_PROTOCOL_V3, IpcChannelV3, IpcEnvelopeV3,
        RuntimeEventV3, TrainerStreamKindV3,
    };
    use crate::{
        control::{
            ControlCommand, ControlRequest, ControlResponse, ControlSnapshot, RuntimeModeState,
        },
        ipc_v2::RuntimeMlKindV2,
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
    fn control_request_roundtrip_via_envelope() {
        let request = ControlRpcRequestV3::from(ControlRequest {
            request_id: Some("req-1".to_string()),
            command: ControlCommand::Snapshot,
        });
        let envelope = IpcEnvelopeV3::new(
            IpcChannelV3::ControlRpc,
            "request",
            1,
            request.request_id.clone(),
            None,
            &request,
        )
        .expect("envelope encoding should succeed");
        assert_eq!(envelope.v, IPC_PROTOCOL_V3);

        let decoded: ControlRpcRequestV3 = envelope
            .decode_payload()
            .expect("payload decode should succeed");
        assert_eq!(decoded, request);
    }

    #[test]
    fn control_response_mapping_keeps_shape() {
        let response = ControlResponse::snapshot(Some("x".to_string()), sample_snapshot());
        let v3 = ControlRpcResponseV3::from(response.clone());
        let roundtrip = ControlResponse::from(v3);
        assert_eq!(roundtrip, response);
    }

    #[test]
    fn trainer_kind_maps_from_v2() {
        let kind = TrainerStreamKindV3::from(RuntimeMlKindV2::TrainerStatus);
        assert_eq!(kind, TrainerStreamKindV3::TrainerStatus);
    }

    #[test]
    fn runtime_event_snapshot_serializes() {
        let event = RuntimeEventV3::Snapshot {
            snapshot: sample_snapshot(),
        };
        let encoded = serde_json::to_string(&event).expect("event json should encode");
        assert!(encoded.contains("\"type\":\"snapshot\""));
    }
}
