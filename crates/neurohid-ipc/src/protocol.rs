//! IPC transport configuration and v3 message contract exports.

pub use neurohid_types::ipc_v2::{
    AckV2, CandidateModelReadyV2, DecisionEventV2, ErrpResultV2, ErrpWindowV2, HelloV2, PingV2,
    PongV2, ProtocolErrorV2, RuntimeMlRoleV2, RuntimeTelemetryV2, SessionBoundaryEventV2,
    SessionBoundaryV2, ShutdownV2, TrainerStatusV2,
};
pub use neurohid_types::ipc_v3::{
    ControlRpcRequestV3, ControlRpcResponsePayloadV3, ControlRpcResponseV3, IPC_PROTOCOL_V3,
    IpcChannelV3, IpcEnvelopeV3, RuntimeComponentCapabilityV3, RuntimeEventV3,
    RuntimeEventsSubscribeV3, TrainerStreamKindV3, TrainerStreamPayloadV3,
};

/// Default TCP port for loopback fallback mode.
pub const DEFAULT_IPC_PORT: u16 = 47_384;
/// Default local-socket endpoint for the unified IPC v3 listener.
pub const DEFAULT_IPC_SOCKET_ENDPOINT: &str = "neurohid.control.v3";
/// Compatibility alias retained for older references.
pub const DEFAULT_RUNTIME_SOCKET_ENDPOINT: &str = DEFAULT_IPC_SOCKET_ENDPOINT;
/// Compatibility alias retained for older references.
pub const DEFAULT_CONTROL_SOCKET_ENDPOINT: &str = DEFAULT_IPC_SOCKET_ENDPOINT;

/// Transport mode for IPC endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IpcTransport {
    /// Cross-platform local socket (Unix domain socket / Windows named pipe).
    #[default]
    LocalSocket,
    /// TCP localhost fallback mode.
    TcpLoopback,
}

/// Configuration for one IPC endpoint.
#[derive(Debug, Clone)]
pub struct IpcConfig {
    /// Transport mode.
    pub transport: IpcTransport,
    /// Endpoint name/path (local socket) or `host:port` address (tcp).
    pub endpoint: String,
    /// Timeout for connection attempts in milliseconds.
    pub connect_timeout_ms: u64,
    /// Timeout for individual message sends in milliseconds.
    pub send_timeout_ms: u64,
    /// Timeout for message receives in milliseconds.
    pub recv_timeout_ms: u64,
    /// Maximum accepted payload size in bytes.
    pub max_message_size: usize,
    /// Capacity for inbound/outbound channel buffering.
    pub channel_capacity: usize,
    /// Whether reconnect loops should be enabled by higher-level clients.
    pub auto_reconnect: bool,
}

/// Queue overflow behavior for brokered channel traffic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueOverflowPolicy {
    /// Reject newly queued work when capacity is exhausted.
    RejectNew,
    /// Stall producer until capacity becomes available and emit a warning.
    StallWarn,
    /// Drop oldest buffered entries to make room for newer traffic.
    DropOldest,
}

/// Queue policy for a specific logical IPC channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChannelPolicy {
    /// Maximum buffered items.
    pub capacity: usize,
    /// Overflow behavior when `capacity` is reached.
    pub overflow: QueueOverflowPolicy,
}

impl ChannelPolicy {
    /// Build a new channel policy.
    pub const fn new(capacity: usize, overflow: QueueOverflowPolicy) -> Self {
        Self { capacity, overflow }
    }
}

/// Broker queue and session policy configuration.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BrokerConfig {
    /// `control.rpc` queue policy.
    pub control: ChannelPolicy,
    /// `trainer.stream` queue policy.
    pub trainer: ChannelPolicy,
    /// `runtime.events` per-subscriber queue policy.
    pub runtime_events: ChannelPolicy,
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            control: ChannelPolicy::new(256, QueueOverflowPolicy::RejectNew),
            trainer: ChannelPolicy::new(1_024, QueueOverflowPolicy::StallWarn),
            runtime_events: ChannelPolicy::new(4_096, QueueOverflowPolicy::DropOldest),
        }
    }
}

impl IpcConfig {
    /// Build config for runtime trainer stream endpoint.
    pub fn runtime_stream_default() -> Self {
        Self {
            endpoint: default_ipc_endpoint(),
            ..Self::default()
        }
    }

    /// Build config for control RPC endpoint.
    pub fn control_default() -> Self {
        Self {
            endpoint: default_ipc_endpoint(),
            ..Self::default()
        }
    }
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            transport: IpcTransport::LocalSocket,
            endpoint: default_ipc_endpoint(),
            connect_timeout_ms: 5_000,
            send_timeout_ms: 250,
            recv_timeout_ms: 250,
            max_message_size: 2 * 1024 * 1024,
            channel_capacity: 128,
            auto_reconnect: true,
        }
    }
}

/// Returns the default runtime stream endpoint.
pub fn default_runtime_endpoint() -> String {
    default_ipc_endpoint()
}

/// Returns the default control RPC endpoint.
pub fn default_control_endpoint() -> String {
    default_ipc_endpoint()
}

/// Returns the canonical unified IPC v3 endpoint.
pub fn default_ipc_endpoint() -> String {
    DEFAULT_IPC_SOCKET_ENDPOINT.to_string()
}

/// Returns the default loopback endpoint.
pub fn default_loopback_endpoint() -> String {
    format!("127.0.0.1:{DEFAULT_IPC_PORT}")
}
