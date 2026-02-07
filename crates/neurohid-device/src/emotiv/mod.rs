//! # Emotiv Cortex API Device Backend
//!
//! Feature-gated module providing support for Emotiv EEG headsets
//! (Insight, EPOC+, EPOC X) through the Cortex WebSocket API.
//!
//! Enable with: `cargo build --features emotiv`
//!
//! ## Prerequisites
//!
//! 1. Install [Emotiv Launcher](https://www.emotiv.com/emotiv-launcher/)
//! 2. Register an application at the [Emotiv Developer Portal](https://www.emotiv.com/developer/)
//!    to obtain a `client_id` and `client_secret`
//! 3. Store the credentials in the platform keychain (handled by `neurohid-core`)
//! 4. Ensure the Cortex service is running (starts with Emotiv Launcher)
//!
//! ## Architecture
//!
//! ```text
//! Emotiv Cortex Service (wss://localhost:6868)
//!     ↕ WebSocket + JSON-RPC 2.0
//! CortexClient (TLS, self-signed cert)
//!     ↕ authenticate / queryHeadsets / createSession / subscribe
//! EmotivProvider (implements DeviceProvider)
//!     ↕ discover / connect
//! EmotivDevice (implements Device)
//!     ↕ start_streaming → spawns tokio task → mpsc → SampleStream
//! NeuroHID pipeline
//! ```

pub mod cortex_client;
pub mod device;
pub mod protocol;
pub mod provider;

pub use device::EmotivDevice;
pub use provider::EmotivProvider;

// Re-export Emotiv-specific data types used by the stream subscriptions,
// session management, and BCI/training APIs.
pub use protocol::{
    BandPowerData, DetectionInfo, DetectionType, DeviceQuality, ExportFormat, FacialExpression,
    MarkerInfo, MentalCommand, MotionData, PerformanceMetrics, ProfileAction, ProfileInfo,
    RecordInfo, TrainingStatus,
};
