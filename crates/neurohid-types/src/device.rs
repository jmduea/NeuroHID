//! # Device Types
//!
//! Types related to biosensor devices: identifiers, connection state,
//! and device information.

use crate::signal::DeviceChannelConfig;
use serde::{Deserialize, Serialize};

/// Unique identifier for a device.
/// This is typically the device's serial number or Bluetooth address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub String);

impl DeviceId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// TODO: Hardcode only devices that we have an integration for, otherwise discover device name/type dynamically.
/// The type/model of a device.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceType {
    /// Emotiv Insight (5-channel consumer EEG)
    EmotivInsight,
    /// Emotiv EPOC+ (14-channel)
    EmotivEpocPlus,
    /// Emotiv EPOC X (14-channel, newer)
    EmotivEpocX,
    /// OpenBCI Cyton (8-channel, expandable)
    OpenBCICyton,
    /// OpenBCI Ganglion (4-channel)
    OpenBCIGanglion,
    /// Muse 2 (4-channel consumer)
    Muse2,
    /// Mock device for testing
    Mock,
    /// Unknown or unsupported device
    Unknown(String),
}

impl DeviceType {
    /// TODO: Could be derived from device metadata instead of hardcoded, let integrations provide this info if we want to hardcode it.
    /// Get the expected channel count for this device type
    pub fn expected_channel_count(&self) -> Option<usize> {
        match self {
            DeviceType::EmotivInsight => Some(5),
            DeviceType::EmotivEpocPlus => Some(14),
            DeviceType::EmotivEpocX => Some(14),
            DeviceType::OpenBCICyton => Some(8), // Can be expanded to 16
            DeviceType::OpenBCIGanglion => Some(4),
            DeviceType::Muse2 => Some(4),
            DeviceType::Mock => None, // Configurable
            DeviceType::Unknown(_) => None,
        }
    }
    /// TODO: Same as above. Derive if possible or let integrations provide.
    /// Get the expected sampling rate for this device type
    pub fn expected_sampling_rate(&self) -> Option<f32> {
        match self {
            DeviceType::EmotivInsight => Some(128.0),
            DeviceType::EmotivEpocPlus => Some(128.0),
            DeviceType::EmotivEpocX => Some(256.0),
            DeviceType::OpenBCICyton => Some(250.0),
            DeviceType::OpenBCIGanglion => Some(200.0),
            DeviceType::Muse2 => Some(256.0),
            DeviceType::Mock => None,
            DeviceType::Unknown(_) => None,
        }
    }
}

/// Information about a discovered or connected device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// The unique identifier for this device
    pub id: DeviceId,

    /// The device type/model
    pub device_type: DeviceType,

    /// Human-readable name (e.g., "John's Insight")
    pub name: Option<String>,

    /// Firmware version, if known
    pub firmware_version: Option<String>,

    /// Channel configuration
    pub channel_config: Option<DeviceChannelConfig>,

    /// Battery level as a percentage (0-100), if available
    pub battery_percent: Option<u8>,

    /// Source identifier for grouping streams from the same physical device.
    ///
    /// Multi-stream publishers (e.g., Emotiv) share a single `source_id`
    /// across all their LSL streams. The UI uses this to group streams
    /// under a single device header.
    pub source_id: Option<String>,
}

/// A discovered LSL stream available on the network.
///
/// This is a lightweight, UI-friendly representation used for stream
/// discovery and management. Unlike `DeviceInfo`, it doesn't carry
/// channel configuration details — those are populated after connection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredStream {
    /// Unique identifier (source_id or stream name if source_id is empty)
    pub id: String,
    /// Human-readable stream name
    pub name: String,
    /// Stream type (e.g., "EEG", "Motion", "Markers")
    pub stream_type: String,
    /// Number of channels in this stream
    pub channel_count: i32,
    /// Nominal sampling rate in Hz
    pub sample_rate: f64,
    /// Whether we currently have an active inlet for this stream
    pub connected: bool,
    /// Current battery level (0-100), if reported by the device
    pub battery_percent: Option<u8>,
    /// Per-channel signal quality (0.0 = bad, 1.0 = good), if available
    pub channel_quality: Option<Vec<f32>>,
    /// Source identifier for grouping streams from the same physical device.
    /// `None` for standalone streams or mock devices.
    pub source_id: Option<String>,
    /// Effective sample rate computed from runtime timestamps.
    pub effective_sample_rate_hz: Option<f64>,
    /// Samples received by the runtime for this stream.
    pub samples_received: Option<u64>,
    /// Samples inferred as dropped (for example by sequence gaps).
    pub samples_dropped: Option<u64>,
    /// Percentage of dropped samples.
    pub drop_rate_pct: Option<f32>,
    /// Age of the most recent sample in milliseconds.
    pub last_sample_age_ms: Option<u64>,
    /// Human-readable summary of active preprocessing for this stream.
    pub preprocessing_summary: Option<String>,
    /// Human-readable integrity state (`ok`, `degraded`, etc.).
    pub integrity_state: Option<String>,
}

/// The current connection state of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    /// Device is not connected
    Disconnected,
    /// Attempting to connect
    Connecting,
    /// Connected and ready to stream
    Connected,
    /// Connection was lost unexpectedly
    ConnectionLost,
    /// An error occurred
    Error,
}

impl ConnectionState {
    /// Check if the device is in a usable state
    pub fn is_usable(&self) -> bool {
        matches!(self, ConnectionState::Connected)
    }

    /// Check if the device is in a transitional state
    pub fn is_transitioning(&self) -> bool {
        matches!(self, ConnectionState::Connecting)
    }
}

/// Overall status of a device, combining connection state with data quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    /// The device this status refers to
    pub device_id: DeviceId,

    /// Current connection state
    pub connection_state: ConnectionState,

    /// Whether data is currently flowing
    pub is_streaming: bool,

    /// Number of samples received since connection
    pub samples_received: u64,

    /// Number of samples dropped/missed (detected via sequence gaps)
    pub samples_dropped: u64,

    /// Current battery level (if available)
    pub battery_percent: Option<u8>,

    /// Per-channel signal quality (0.0 = bad, 1.0 = good)
    pub channel_quality: Option<Vec<f32>>,

    /// Human-readable status message
    pub message: Option<String>,
}

impl DeviceStatus {
    /// Create a new disconnected status
    pub fn disconnected(device_id: DeviceId) -> Self {
        Self {
            device_id,
            connection_state: ConnectionState::Disconnected,
            is_streaming: false,
            samples_received: 0,
            samples_dropped: 0,
            battery_percent: None,
            channel_quality: None,
            message: None,
        }
    }

    /// Get the sample drop rate as a percentage
    pub fn drop_rate(&self) -> f32 {
        let total = self.samples_received + self.samples_dropped;
        if total == 0 {
            0.0
        } else {
            (self.samples_dropped as f32 / total as f32) * 100.0
        }
    }

    /// Get the average channel quality
    pub fn average_quality(&self) -> Option<f32> {
        self.channel_quality.as_ref().map(|q| {
            if q.is_empty() {
                0.0
            } else {
                q.iter().sum::<f32>() / q.len() as f32
            }
        })
    }
}

/// Settings for device connection behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionSettings {
    /// Whether to automatically reconnect if connection is lost
    pub auto_reconnect: bool,

    /// Maximum number of reconnection attempts
    pub max_reconnect_attempts: u32,

    /// Delay between reconnection attempts in milliseconds
    pub reconnect_delay_ms: u64,

    /// Timeout for connection attempts in milliseconds
    pub connection_timeout_ms: u64,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            auto_reconnect: true,
            max_reconnect_attempts: 5,
            reconnect_delay_ms: 1000,
            connection_timeout_ms: 10000,
        }
    }
}
