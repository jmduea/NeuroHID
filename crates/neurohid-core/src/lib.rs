//! # NeuroHID Core Library
//!
//! This library exposes the NeuroHID service for embedding into other binaries
//! (e.g., the hub GUI). The standalone headless binary (`neurohid-service`)
//! also imports from here.
//!
//! ## Usage
//!
//! ```no_run
//! use neurohid_core::service::{NeuroHidService, ServiceHandle};
//! ```

pub mod extension_registry;
pub mod observability;
pub mod recording;
pub mod runtime;
pub mod service;
/// Internal pipeline task implementations.
///
/// Not part of the stable embedder API. Use [`runtime::RuntimeBuilder`] and
/// [`runtime::RuntimeHandle`] instead. This module is hidden from documentation
/// and may change without notice.
#[doc(hidden)]
pub mod tasks;

/// Re-exports from downstream crates used by the hub and other embedders.
///
/// Consumers that embed the runtime through [`runtime::RuntimeHandle`] can
/// import IPC and storage helpers from here instead of depending on the
/// component crates directly.
pub mod facade {
    // IPC types needed by hub external-mode control path.
    pub use neurohid_ipc::{IpcClient, IpcConfig, IpcTransport, send_control_request_blocking};

    // Storage types needed by hub initialization and state.
    pub use neurohid_storage::{ConfigStore, DataPaths, ProfileStore, SecureStorage, initialize};
}
