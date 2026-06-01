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
//! - **Serial** — Reads samples from direct USB/UART ports using configurable
//!   framing (`csv_line` or `binary_i16_le`).
//! - **Mock Device** — Always available, for testing and development without hardware.
//!
//! ## Architecture
//!
//! The device layer is organized around two main traits:
//!
//! - [`DeviceProvider`]: Handles discovery and connection establishment
//! - [`Device`]: Represents a connected device and handles streaming
//!
//! For the LSL backend specifically, these framework traits map to stream-native
//! semantics:
//! - `DeviceProvider::discover/connect` == resolve stream/open inlet
//! - `Device::start_streaming` == pull samples from that inlet
//!
//! The crate exposes stream-native aliases for clarity:
//! - [`LslStreamResolver`] (alias of [`LslProvider`])
//! - [`LslInletClient`] (alias of [`LslDevice`])
//!
//! Consumers of this crate work with them uniformly through the trait interface.

#[cfg(feature = "brainflow")]
pub mod brainflow;
#[cfg(all(feature = "brainflow", feature = "brainflow-native"))]
mod brainflow_native;
#[cfg(feature = "lsl")]
pub mod lsl;
pub mod mock;
pub mod serial;
pub mod traits;

#[cfg(feature = "brainflow")]
pub use brainflow::{BrainFlowDevice, BrainFlowProvider};
#[cfg(feature = "lsl")]
pub use lsl::{LslDevice, LslInletClient, LslProvider, LslStreamResolver};
pub use mock::MockDeviceConfig;
pub use serial::{SerialDevice, SerialProvider};
pub use traits::{Device, DeviceExt, DeviceProvider, SampleStream};

// Re-export commonly used types from neurohid-types for convenience
pub use neurohid_types::device::{
    ConnectionSettings, ConnectionState, DeviceId, DeviceInfo, DeviceStatus, DeviceType,
};
pub use neurohid_types::error::{DeviceError, Result};
pub use neurohid_types::signal::{ChannelConfig, ChannelId, DeviceChannelConfig, Sample};
