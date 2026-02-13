//! # IPC Protocol
//!
//! This module defines the protocol for communication between the Rust core
//! service and the Python ML layer. The two processes communicate over a TCP
//! socket on localhost (127.0.0.1).
//!
//! ## Why IPC Instead of FFI?
//!
//! We chose IPC over direct FFI (like PyO3) for several reasons:
//!
//! 1. **Process isolation**: If Python crashes (OOM, segfault), the Rust service
//!    continues running. The user's input doesn't suddenly stop.
//!
//! 2. **Language flexibility**: We can swap Python for Julia or another ML runtime
//!    without changing the Rust code.
//!
//! 3. **Debugging**: We can inspect, log, and replay IPC messages. Much harder
//!    with in-process FFI.
//!
//! 4. **GIL avoidance**: Python's GIL doesn't affect Rust's performance since
//!    they're in separate processes.
//!
//! 5. **Hot reloading**: We can restart the Python process to pick up ML code
//!    changes without restarting the whole system.
//!
//! The main downside is latency overhead (~0.1-0.5ms per message), but our
//! messages are small and infrequent enough that this doesn't matter.
//!
//! ## Protocol Overview
//!
//! ```text
//! Rust Core                              Python ML
//! ─────────                              ─────────
//!     │                                      │
//!     │──── FeatureBatch ────────────────────>│
//!     │     (features @ 20-60Hz)             │
//!     │                                      │
//!     │<───────────────────── Action ────────│
//!     │     (decoded action)                 │
//!     │                                      │
//!     │──── ErrPWindow ──────────────────────>│
//!     │     (signal window for ErrP check)   │
//!     │                                      │
//!     │<───────────────────── ErrPResult ────│
//!     │     (error probability)              │
//!     │                                      │
//!     │──── TrainingBatch ───────────────────>│
//!     │     (experience replay data)         │
//!     │                                      │
//! ```
//!
//! ## Message Framing
//!
//! Messages are framed with a simple length prefix:
//! - 4 bytes: message length (little-endian u32)
//! - N bytes: JSON-encoded message
//!
//! We use JSON for debuggability. If profiling shows this is a bottleneck,
//! we can switch to MessagePack or a binary format.

use neurohid_types::{
    action::Action,
    config::ServiceState,
    reward::{ErrPResult, SignalQuality},
    signal::FeatureVector,
    Timestamp,
};
use serde::{Deserialize, Serialize};

/// Messages sent from Rust to Python.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RustToPython {
    /// A batch of features ready for decoding.
    /// Sent continuously at the feature extraction rate (e.g., 20Hz).
    FeatureBatch {
        /// The feature vectors to decode
        features: Vec<FeatureVector>,
        /// Current observation context (cursor state, etc.)
        context: ObservationContext,
        /// Sequence number for ordering/deduplication
        sequence: u64,
    },

    /// A window of signal data for ErrP detection.
    /// Sent after each action, with appropriate delay for ErrP to appear.
    ErrPWindow {
        /// The action timestamp this window corresponds to
        action_timestamp: Timestamp,
        /// The action that was taken
        action: Action,
        /// Raw signal data in the ErrP window (features, not raw samples)
        window_features: Vec<FeatureVector>,
        /// Sequence number
        sequence: u64,
    },

    /// Training data for the decoder (experience replay).
    /// Sent periodically when there's enough data for a training batch.
    TrainingBatch {
        /// Observations
        observations: Vec<FeatureVector>,
        /// Actions taken
        actions: Vec<Action>,
        /// Rewards received (from ErrP)
        rewards: Vec<f32>,
        /// Whether each step is terminal
        dones: Vec<bool>,
        /// Sequence number
        sequence: u64,
    },

    /// Request for Python to update its model weights.
    /// Sent after calibration or significant online learning.
    ModelUpdate {
        /// Path to the new model file
        model_path: String,
        /// What kind of model (decoder, errp)
        model_type: ModelType,
    },

    /// Service status update.
    StatusUpdate { state: ServiceState },

    /// Request to shutdown cleanly.
    Shutdown,

    /// Ping for health checking.
    Ping { timestamp: Timestamp },
}

/// Messages sent from Python to Rust.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PythonToRust {
    /// A decoded action to execute.
    /// This is the primary output of the decoder.
    Action {
        /// The action to take
        action: Action,
        /// Which feature batch this corresponds to
        sequence: u64,
        /// Inference latency in microseconds
        inference_latency_us: i64,
    },

    /// Result of ErrP detection.
    ErrPResult {
        /// The detection result
        result: ErrPResult,
        /// Which ErrP window this corresponds to
        sequence: u64,
    },

    /// Training completed notification.
    TrainingComplete {
        /// Which batch was trained on
        sequence: u64,
        /// Training metrics
        metrics: TrainingMetrics,
    },

    /// Model loaded successfully.
    ModelLoaded {
        model_type: ModelType,
        /// Model metadata (architecture, param count, etc.)
        metadata: ModelMetadata,
    },

    /// An error occurred in Python.
    Error {
        /// Error message
        message: String,
        /// Whether Python can continue operating
        recoverable: bool,
    },

    /// Response to ping.
    Pong {
        /// Echo back the timestamp from ping
        timestamp: Timestamp,
        /// Python's current timestamp
        python_timestamp: Timestamp,
    },

    /// Python is ready to receive messages.
    Ready,
}

/// Additional context sent with feature batches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationContext {
    /// Current cursor position (normalized)
    pub cursor_x: f32,
    pub cursor_y: f32,

    /// Cursor velocity
    pub cursor_velocity_x: f32,
    pub cursor_velocity_y: f32,

    /// Screen dimensions
    pub screen_width: u32,
    pub screen_height: u32,

    /// Current signal quality
    pub signal_quality: SignalQuality,

    /// Timestamp of this context
    pub timestamp: Timestamp,
}

/// Types of models that can be updated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelType {
    /// The decoder (RL policy)
    Decoder,
    /// The ErrP classifier
    ErrP,
}

/// Metrics from a training step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingMetrics {
    /// Policy loss
    pub policy_loss: f32,
    /// Value loss
    pub value_loss: f32,
    /// Entropy
    pub entropy: f32,
    /// Approximate KL divergence
    pub approx_kl: f32,
    /// Number of samples in batch
    pub batch_size: usize,
    /// Training duration in microseconds
    pub duration_us: i64,
}

/// Metadata about a loaded model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// Model architecture name
    pub architecture: String,
    /// Number of parameters
    pub param_count: usize,
    /// Input dimension
    pub input_dim: usize,
    /// Output dimension
    pub output_dim: usize,
    /// Any additional info
    pub extra: Option<String>,
}

/// Default IPC port for TCP localhost communication.
pub const DEFAULT_IPC_PORT: u16 = 47384;

/// Configuration for the IPC connection.
#[derive(Debug, Clone)]
pub struct IpcConfig {
    /// TCP address to bind/connect (e.g., "127.0.0.1:47384")
    pub address: String,

    /// Timeout for connection attempts in milliseconds
    pub connect_timeout_ms: u64,

    /// Timeout for individual message sends in milliseconds
    pub send_timeout_ms: u64,

    /// Timeout for message receives in milliseconds
    pub recv_timeout_ms: u64,

    /// Maximum message size in bytes
    pub max_message_size: usize,

    /// Whether to reconnect automatically if connection is lost
    pub auto_reconnect: bool,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            address: default_address(),
            connect_timeout_ms: 5000,
            send_timeout_ms: 100,
            recv_timeout_ms: 100,
            max_message_size: 1024 * 1024, // 1 MB
            auto_reconnect: true,
        }
    }
}

/// Returns the default TCP address for IPC communication.
pub fn default_address() -> String {
    format!("127.0.0.1:{}", DEFAULT_IPC_PORT)
}
