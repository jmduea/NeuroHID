//! # IPC Transport Protocol
//!
//! Transport-level configuration and message contract for runtime ML bridge
//! communication.

pub use neurohid_types::ipc_v2::{
    AckV2, CandidateModelReadyV2, DecisionEventV2, ErrpResultV2, ErrpWindowV2, HelloV2, PingV2,
    PongV2, ProtocolErrorV2, RUNTIME_ML_PROTOCOL_V2, RuntimeMlEnvelopeV2, RuntimeMlKindV2,
    RuntimeMlRoleV2, RuntimeTelemetryV2, SessionBoundaryEventV2, SessionBoundaryV2, ShutdownV2,
    TrainerStatusV2,
};
use serde::{Deserialize, Serialize};

/// Default IPC port for localhost TCP fallback.
pub const DEFAULT_IPC_PORT: u16 = 47_384;
/// Default named pipe for runtime ML bridge.
pub const DEFAULT_ML_PIPE_NAME: &str = r"\\.\pipe\neurohid.ml.v2";

/// Transport mode for runtime ML IPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IpcTransport {
    /// Windows named pipe transport.
    #[default]
    NamedPipe,
    /// Localhost TCP fallback.
    TcpLoopback,
}

/// Configuration for IPC connection transport.
#[derive(Debug, Clone)]
pub struct IpcConfig {
    /// Transport mode.
    pub transport: IpcTransport,
    /// TCP address used in loopback mode.
    pub address: String,
    /// Named pipe path used in named-pipe mode.
    pub pipe_name: String,
    /// Timeout for connection attempts in milliseconds.
    pub connect_timeout_ms: u64,
    /// Timeout for individual message sends in milliseconds.
    pub send_timeout_ms: u64,
    /// Timeout for message receives in milliseconds.
    pub recv_timeout_ms: u64,
    /// Maximum accepted message size in bytes.
    pub max_message_size: usize,
    /// Whether to reconnect automatically if connection is lost.
    pub auto_reconnect: bool,
}

impl Default for IpcConfig {
    fn default() -> Self {
        #[cfg(windows)]
        let transport = IpcTransport::NamedPipe;
        #[cfg(not(windows))]
        let transport = IpcTransport::TcpLoopback;

        Self {
            transport,
            address: default_address(),
            pipe_name: DEFAULT_ML_PIPE_NAME.to_string(),
            connect_timeout_ms: 5_000,
            send_timeout_ms: 100,
            recv_timeout_ms: 100,
            max_message_size: 1024 * 1024,
            auto_reconnect: true,
        }
    }
}

/// Returns the default TCP address for IPC loopback.
pub fn default_address() -> String {
    format!("127.0.0.1:{DEFAULT_IPC_PORT}")
}
