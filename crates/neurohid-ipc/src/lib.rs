//! # NeuroHID IPC Layer
//!
//! This crate provides inter-process communication between the Rust core service
//! and the trainer bridge process. Transport is named pipes on Windows (default)
//! with optional localhost TCP fallback for non-Windows development.
//!
//! ## Architecture
//!
//! The Rust core runs continuously as a background service. It connects to the
//! EEG device, processes signals, and emits HID events. The Python process runs
//! alongside, receiving feature vectors and returning decoded actions.
//!
//! ```text
//!                 ┌─────────────────────────────────┐
//!                 │         Rust Core Service       │
//!                 │  ┌─────────┐    ┌───────────┐  │
//!   EEG Device ───│─>│ Signal  │───>│ IPC Server│──│──┐
//!                 │  │ Pipeline│    └───────────┘  │  │
//!                 │  └─────────┘          ▲        │  │
//!                 │       │               │        │  │
//!                 │       ▼               │        │  │
//!                 │  ┌─────────┐    ┌─────┴─────┐  │  │
//!   HID Output <──│──│ Platform│<───│Action     │  │  │
//!                 │  │ Layer   │    │Executor   │  │  │
//!                 │  └─────────┘    └───────────┘  │  │
//!                 └─────────────────────────────────┘  │
//!                                                      │ Local Socket
//!                 ┌─────────────────────────────────┐  │
//!                 │       Python ML Process         │  │
//!                 │  ┌───────────┐    ┌─────────┐  │  │
//!                 │  │IPC Client │<───│         │  │<─┘
//!                 │  └─────┬─────┘    │ Decoder │  │
//!                 │        │          │ (PyTorch│  │
//!                 │        ▼          │  PPO)   │  │
//!                 │  ┌───────────┐    │         │  │
//!                 │  │   ErrP    │───>│         │  │
//!                 │  │ Detector  │    └─────────┘  │
//!                 │  └───────────┘                 │
//!                 └─────────────────────────────────┘
//! ```
//!
//! ## Usage (Rust Side)
//!
//! ```ignore
//! use neurohid_ipc::{IpcConfig, IpcServer, RuntimeMlEnvelopeV2, RuntimeMlKindV2};
//!
//! // Start the IPC server
//! let server = IpcServer::new(IpcConfig::default()).await?;
//!
//! // Wait for trainer bridge to connect
//! let connection = server.accept().await?;
//!
//! // Send decision event envelope
//! let msg = RuntimeMlEnvelopeV2::new(RuntimeMlKindV2::DecisionEvent, 1, "session", &payload)?;
//! connection.send(msg).await?;
//!
//! // Receive a reply envelope
//! let msg = connection.recv().await?;
//! ```

pub mod client;
pub mod protocol;
pub mod server;

pub use protocol::{
    default_address, AckV2, CandidateModelReadyV2, DecisionEventV2, ErrpResultV2, ErrpWindowV2,
    HelloV2, IpcConfig, IpcTransport, PingV2, PongV2, ProtocolErrorV2, RuntimeMlEnvelopeV2,
    RuntimeMlKindV2, RuntimeMlRoleV2, RuntimeTelemetryV2, SessionBoundaryEventV2,
    SessionBoundaryV2, ShutdownV2, TrainerStatusV2, DEFAULT_IPC_PORT, DEFAULT_ML_PIPE_NAME,
    RUNTIME_ML_PROTOCOL_V2,
};

// Server is used by Rust core
pub use server::IpcServer;

// Client would be used by Python (via PyO3 bindings) or for testing
pub use client::IpcClient;
