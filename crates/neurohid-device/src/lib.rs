//! # NeuroHID Device Abstraction
//!
//! This crate provides the device abstraction layer for NeuroHID. It defines
//! traits for discovering, connecting to, and streaming from biosensor devices,
//! along with backend implementations.
//!
//! ## Supported Backends
//!
//! - **LSL** — Consumes any [Lab Streaming Layer](https://labstreaminglayer.readthedocs.io/)
//!   stream on the local network. Device-specific software (emotiv-cortex-cli,
//!   MuseLSL, OpenBCI GUI, etc.) pushes data into LSL; this adapter pulls it in.
//! - **Mock Device** — Always available, for testing and development without hardware.
//!
//! ## Architecture
//!
//! The device layer is organized around two main traits:
//!
//! - [`DeviceProvider`]: Handles discovery and connection establishment
//! - [`Device`]: Represents a connected device and handles streaming
//!
//! Consumers of this crate work with them uniformly through the trait interface.

pub mod traits;
pub mod mock;
pub mod lsl;

pub use traits::{Device, DeviceProvider, DeviceExt, SampleStream};
pub use mock::MockDeviceConfig;
pub use lsl::{LslProvider, LslDevice};

// Re-export commonly used types from neurohid-types for convenience
pub use neurohid_types::device::{
    DeviceId, DeviceInfo, DeviceStatus, DeviceType, ConnectionState, ConnectionSettings,
};
pub use neurohid_types::signal::{Sample, DeviceChannelConfig, ChannelConfig, ChannelId};
pub use neurohid_types::error::{DeviceError, Result};
