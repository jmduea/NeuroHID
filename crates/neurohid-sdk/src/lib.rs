//! # NeuroHID SDK
//!
//! A feature-gated facade crate that provides access to NeuroHID's internal
//! libraries. Enable only the features you need to keep compile times fast
//! and dependency trees minimal.
//!
//! ## Features
//!
//! | Feature | Crate | Description |
//! |---------|-------|-------------|
//! | `types` (default) | `neurohid-types` | Core type definitions (signals, actions, devices) |
//! | `signal` | `neurohid-signal` | Real-time biosignal processing pipeline |
//! | `device` | `neurohid-device` | Device abstraction layer for biosensors |
//! | `device-lsl` | `neurohid-device` + LSL | Device layer with Lab Streaming Layer support |
//! | `platform` | `neurohid-platform` | Cross-platform HID emulation |
//! | `storage` | `neurohid-storage` | Secure profile and config storage |
//! | `ipc` | `neurohid-ipc` | IPC layer for Rust↔Python communication |
//! | `calibration` | `neurohid-calibration` | Calibration games and wizard |
//! | `full` | All of the above | Everything enabled |
//!
//! ## Quick Start
//!
//! ```toml
//! [dependencies]
//! neurohid-sdk = { version = "0.1", features = ["device", "signal"] }
//! ```
//!
//! ```rust,ignore
//! use neurohid_sdk::types;
//! use neurohid_sdk::device;
//! use neurohid_sdk::signal;
//! ```

#[cfg(feature = "types")]
pub use neurohid_types as types;

#[cfg(feature = "signal")]
pub use neurohid_signal as signal;

#[cfg(feature = "device")]
pub use neurohid_device as device;

#[cfg(feature = "platform")]
pub use neurohid_platform as platform;

#[cfg(feature = "storage")]
pub use neurohid_storage as storage;

#[cfg(feature = "ipc")]
pub use neurohid_ipc as ipc;

#[cfg(feature = "calibration")]
pub use neurohid_calibration as calibration;
