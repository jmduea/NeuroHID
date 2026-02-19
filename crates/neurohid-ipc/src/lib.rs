//! NeuroHID IPC transport and protocol adapters.
//!
//! This crate wraps `ipckit` local-socket primitives plus loopback TCP fallback
//! to provide a unified framed-JSON transport for NeuroHID IPC v3.

pub mod broker;
pub mod client;
pub mod protocol;
pub mod server;

pub use broker::{BrokerCounters, BrokerError, IpcBroker, TrainerSessionGuard};
pub use protocol::{
    AckV2, BrokerConfig, CandidateModelReadyV2, ChannelPolicy, ControlRpcRequestV3,
    ControlRpcResponsePayloadV3, ControlRpcResponseV3, DEFAULT_CONTROL_SOCKET_ENDPOINT,
    DEFAULT_IPC_PORT, DEFAULT_IPC_SOCKET_ENDPOINT, DEFAULT_RUNTIME_SOCKET_ENDPOINT,
    DecisionEventV2, ErrpResultV2, ErrpWindowV2, HelloV2, IPC_PROTOCOL_V3, IpcChannelV3, IpcConfig,
    IpcEnvelopeV3, IpcTransport, PingV2, PongV2, ProtocolErrorV2, QueueOverflowPolicy,
    RuntimeComponentCapabilityV3, RuntimeEventV3, RuntimeEventsSubscribeV3, RuntimeMlRoleV2,
    RuntimeTelemetryV2, SessionBoundaryEventV2, SessionBoundaryV2, ShutdownV2, TrainerStatusV2,
    TrainerStreamKindV3, TrainerStreamPayloadV3, default_control_endpoint, default_ipc_endpoint,
    default_loopback_endpoint, default_runtime_endpoint,
};

pub use client::{
    IpcClient, decode_control_response_envelope, send_control_request_blocking,
    send_control_request_once,
};
pub use server::{IpcConnection, IpcServer};
