//! # NeuroHID Core Types
//!
//! This crate defines the fundamental types shared across all NeuroHID components.
//! It deliberately has minimal dependencies to ensure it can be used everywhere
//! without creating circular dependencies.
//!
//! ## Design Philosophy
//!
//! Types are organized into modules by domain:
//! - `signal`: Types related to biosignal data (samples, channels, features)
//! - `action`: Types related to HID output (mouse, keyboard actions)
//! - `device`: Types related to biosensor devices
//! - `observation`: Types for the observation space (what the decoder sees)
//! - `reward`: Types for the reward signal (ErrP-based feedback)
//! - `profile`: Types for user profiles and calibration state
//! - `config`: Configuration types
//!
//! ## Conventions
//!
//! - All timestamps are in microseconds since Unix epoch (i64)
//! - All signal amplitudes are in microvolts (f32)
//! - Coordinates are normalized to [0.0, 1.0] where possible
//! - Errors use `thiserror` for clean error hierarchies

pub mod action;
pub mod config;
pub mod control;
pub mod device;
pub mod error;
pub mod event;
pub mod learning;
pub mod model;
pub mod observation;
pub mod profile;
pub mod reward;
pub mod signal;

// Re-export commonly used types at the crate root for convenience
pub use action::{Action, Key, KeyAction, MouseAction, MouseButton};
pub use control::{
    ControlCommand, ControlRequest, ControlResponse, ControlResponsePayload, ControlSnapshot,
};
pub use device::{ConnectionState, DeviceId, DeviceInfo, DeviceStatus, DiscoveredStream};
pub use error::{Error, Result};
pub use event::{MarkerPayload, MarkerType, StreamMarker};
pub use learning::{
    CandidateGuardrails, CandidateModelMetrics, TrainingEpisode, TrainingSessionLog,
};
pub use model::{ModelManifest, NormalizationStats};
pub use observation::{CursorState, Observation};
pub use profile::{CalibrationState, ProfileId};
pub use reward::{ErrPResult, RewardSignal, SignalQuality};
pub use signal::{ChannelConfig, ChannelId, FeatureVector, Sample};

/// Microseconds since Unix epoch. We use i64 to allow for negative values
/// (timestamps before epoch) even though we don't expect them in practice.
pub type Timestamp = i64;

/// Returns the current timestamp in microseconds since Unix epoch.
pub fn now_micros() -> Timestamp {
    chrono::Utc::now().timestamp_micros()
}
