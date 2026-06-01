//! # Session Recording Types
//!
//! Types for session recording: manifest written to each session folder,
//! and recording configuration used from system config.
//!
//! Session folder layout (see RESEARCH and config-format.md):
//! - `session_<id>/manifest.json` — session identity, start/end, format version
//! - `session_<id>/config.yaml` — snapshot of SystemConfig
//! - `session_<id>/profile_meta.json` — profile metadata (or ref) when profile active
//! - `session_<id>/streams/` — raw stream files (per source or combined)
//! - `session_<id>/actions.jsonl` — one JSON object per line (timestamp, action/decision_id, confidence, etc.)

use serde::{Deserialize, Serialize};

/// Manifest serialized as `manifest.json` in each session folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionManifest {
    /// Unique session identifier (e.g. from now_micros() at start).
    pub session_id: String,
    /// Start time in microseconds since Unix epoch.
    pub started_at_us: i64,
    /// End time when recording stopped; absent while recording is active.
    pub ended_at_us: Option<i64>,
    /// Path or reference to config snapshot (e.g. "config.yaml" in same folder).
    pub config_ref: Option<String>,
    /// Format version of the session layout (e.g. "1").
    pub format_version: String,
    /// Runtime version string, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_version: Option<String>,
    /// SDK or format tool version, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdk_version: Option<String>,
    /// Active profile id when recording started, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    /// Optional device/stream summary for reproducibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_stream_summary: Option<String>,
}

/// When to auto-start/stop recording.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingAutoMode {
    /// No auto start/stop.
    #[default]
    Off,
    /// Start when runtime starts; stop when runtime shuts down.
    TiedToRuntime,
    /// Start when HID output is enabled; stop when output is disabled.
    TiedToOutput,
}

/// Recording configuration used from SystemConfig.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    /// Default output directory for session folders (path string); None = no default path.
    #[serde(default)]
    pub default_output_path: Option<String>,
    /// Auto start/stop behavior.
    #[serde(default)]
    pub auto_mode: RecordingAutoMode,
    /// Optional maximum recording duration in seconds; None = no cap.
    #[serde(default)]
    pub max_duration_secs: Option<u64>,
    /// Optional maximum total size in MB; None = no cap.
    #[serde(default)]
    pub max_size_mb: Option<u64>,
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            default_output_path: None,
            auto_mode: RecordingAutoMode::Off,
            max_duration_secs: None,
            max_size_mb: None,
        }
    }
}
