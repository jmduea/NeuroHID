//! # NeuroHID IPC Layer
//!
//! This crate provides inter-process communication between the Rust core service
//! and the Python ML layer. The two processes communicate over a TCP socket on
//! localhost, with the Rust side acting as the server and Python as the client.
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
//! use neurohid_ipc::{IpcServer, RustToPython, PythonToRust};
//!
//! // Start the IPC server
//! let server = IpcServer::new(IpcConfig::default()).await?;
//!
//! // Wait for Python to connect
//! let connection = server.accept().await?;
//!
//! // Send features
//! connection.send(RustToPython::FeatureBatch { ... }).await?;
//!
//! // Receive actions
//! let msg = connection.recv().await?;
//! if let PythonToRust::Action { action, .. } = msg {
//!     execute_action(action);
//! }
//! ```

pub mod protocol;
pub mod server;
pub mod client;

pub use protocol::{
    RustToPython, PythonToRust,
    ObservationContext, ModelType, TrainingMetrics, ModelMetadata,
    IpcConfig, default_address, DEFAULT_IPC_PORT,
};

// Server is used by Rust core
pub use server::IpcServer;

// Client would be used by Python (via PyO3 bindings) or for testing
pub use client::IpcClient;
