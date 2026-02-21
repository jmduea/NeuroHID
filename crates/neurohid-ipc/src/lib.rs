//! NeuroHID IPC transport and protocol adapters.
//!
//! This crate wraps `ipckit` local-socket primitives plus loopback TCP fallback
//! to provide a unified framed-JSON transport for NeuroHID IPC v3.

pub mod broker;
pub mod client;
pub mod protocol;
pub mod server;
pub mod types;

pub use broker::{BrokerCounters, BrokerError, IpcBroker, TrainerSessionGuard};
pub use protocol::{
    Ack, BrokerConfig, CandidateModelReady, ChannelPolicy, ControlRpcRequest, ControlRpcResponse,
    ControlRpcResponsePayload, DEFAULT_CONTROL_SOCKET_ENDPOINT, DEFAULT_IPC_PORT,
    DEFAULT_IPC_SOCKET_ENDPOINT, DEFAULT_RUNTIME_SOCKET_ENDPOINT, DecisionEvent, ErrpResult,
    ErrpWindow, Hello, IPC_PROTOCOL_VERSION, IpcChannel, IpcConfig, IpcEnvelope, IpcTransport,
    Ping, Pong, ProtocolError, QueueOverflowPolicy, RuntimeComponentCapability, RuntimeEvent,
    RuntimeEventsSubscribe, RuntimeMlRole, RuntimeTelemetry, SessionBoundary, SessionBoundaryEvent,
    Shutdown, TrainerStatus, TrainerStreamKind, TrainerStreamPayload, default_control_endpoint,
    default_ipc_endpoint, default_loopback_endpoint, default_runtime_endpoint,
};

pub use client::{
    IpcClient, decode_control_response_envelope, send_control_request_blocking,
    send_control_request_once,
};
pub use server::{IpcConnection, IpcServer};
