//! # NeuroHID Device Abstraction
//!
//! This crate provides the device abstraction layer for NeuroHID. It defines
//! traits for discovering, connecting to, and streaming from biosensor devices,
//! along with implementations for specific device families.
//!
//! ## Supported Backends
//!
//! - **Mock Device** — Always available, for testing and development without hardware
//! - **BrainFlow** (feature: `brainflow`) — Supports 30+ EEG boards (OpenBCI, Muse, etc.)
//! - **Emotiv Cortex** (feature: `emotiv`) — Emotiv Insight, EPOC+, EPOC X via Cortex API
//!
//! ## Architecture
//!
//! The device layer is organized around two main traits:
//!
//! - [`DeviceProvider`]: Handles discovery and connection establishment
//! - [`Device`]: Represents a connected device and handles streaming
//!
//! Each device family provides its own implementations of these traits,
//! but consumers of this crate can work with them uniformly through the
//! trait interface.
//!
//! ## Feature Flags
//!
//! ```toml
//! [dependencies]
//! neurohid-device = { path = "...", features = ["brainflow", "emotiv"] }
//! ```

pub mod traits;
pub mod mock;

#[cfg(feature = "brainflow")]
pub mod brainflow;

#[cfg(feature = "emotiv")]
pub mod emotiv;

pub use traits::{Device, DeviceProvider, DeviceExt, SampleStream};
pub use mock::MockDeviceConfig;

// Re-export commonly used types from neurohid-types for convenience
pub use neurohid_types::device::{
    DeviceId, DeviceInfo, DeviceStatus, DeviceType, ConnectionState, ConnectionSettings,
};
pub use neurohid_types::signal::{Sample, DeviceChannelConfig, ChannelConfig, ChannelId};
pub use neurohid_types::error::{DeviceError, Result};
