//! # Configuration Types
//!
//! Types defining configuration for all NeuroHID components.
//! Configuration is hierarchical: there's a top-level system config
//! that contains configs for each subsystem.

use crate::{
    action::ActionSpace, device::ConnectionSettings, observation::ObservationConfig,
    reward::ErrPConfig,
};
use serde::{Deserialize, Serialize};

/// Top-level system configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemConfig {
    /// Configuration for device connection
    pub device: DeviceConfig,

    /// Configuration for signal processing
    pub signal: SignalConfig,

    /// Configuration for the observation space
    pub observation: ObservationConfig,

    /// Configuration for ErrP detection
    pub errp: ErrPConfig,

    /// Configuration for the decoder
    pub decoder: DecoderConfig,

    /// Configuration for the action output
    pub action: ActionConfig,

    /// Configuration for profile/storage
    pub storage: StorageConfig,

    /// Configuration for the service itself
    pub service: ServiceConfig,
}

/// Which device backend to use for data acquisition.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceBackend {
    /// Mock device for testing and development (no hardware needed).
    Mock,
    /// Lab Streaming Layer — consume any LSL stream on the local network.
    Lsl,
    /// Auto-detect: try LSL first, then fall back to Mock.
    #[default]
    Auto,
}

impl DeviceBackend {
    /// All variants in display order, for use in UI selectors.
    pub const ALL: &'static [DeviceBackend] = &[
        DeviceBackend::Auto,
        DeviceBackend::Lsl,
        DeviceBackend::Mock,
    ];
}

impl std::fmt::Display for DeviceBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceBackend::Mock => write!(f, "Mock"),
            DeviceBackend::Lsl => write!(f, "LSL"),
            DeviceBackend::Auto => write!(f, "Auto"),
        }
    }
}

/// Configuration for the LSL (Lab Streaming Layer) backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LslConfig {
    /// LSL resolve predicate for stream discovery.
    ///
    /// Examples: `"type='EEG'"`, `"name='EmotivEEG'"`, `""` (all streams).
    /// See LSL docs for predicate syntax.
    #[serde(default)]
    pub predicate: String,

    /// Timeout for stream resolution in seconds.
    #[serde(default = "default_resolve_timeout")]
    pub resolve_timeout_secs: f64,

    /// Inlet buffer size in samples (0 = LSL default of 360 seconds).
    #[serde(default)]
    pub buffer_size: u32,
}

fn default_resolve_timeout() -> f64 {
    1.0
}

impl Default for LslConfig {
    fn default() -> Self {
        Self {
            predicate: String::new(),
            resolve_timeout_secs: default_resolve_timeout(),
            buffer_size: 0,
        }
    }
}

/// Configuration for device connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// Which backend to use for device communication.
    #[serde(default)]
    pub backend: DeviceBackend,

    /// Preferred device type to connect to.
    pub preferred_device_type: Option<String>,

    /// Specific device ID to connect to (if known).
    pub preferred_device_id: Option<String>,

    /// Connection behavior settings.
    pub connection: ConnectionSettings,

    /// LSL-specific configuration (only used when backend is Lsl or Auto).
    #[serde(default)]
    pub lsl: Option<LslConfig>,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            backend: DeviceBackend::default(),
            preferred_device_type: None,
            preferred_device_id: None,
            connection: ConnectionSettings::default(),
            lsl: None,
        }
    }
}

/// Configuration for signal processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    /// Size of the ring buffer in samples
    pub buffer_size_samples: usize,

    /// Whether to apply notch filter (for powerline interference)
    pub notch_filter_enabled: bool,

    /// Notch filter frequency (typically 50Hz or 60Hz depending on region)
    pub notch_filter_hz: f32,

    /// Whether to apply bandpass filter
    pub bandpass_filter_enabled: bool,

    /// Bandpass filter low cutoff (Hz)
    pub bandpass_low_hz: f32,

    /// Bandpass filter high cutoff (Hz)
    pub bandpass_high_hz: f32,

    /// Feature extraction window size in milliseconds
    pub feature_window_ms: u32,

    /// Feature extraction step size in milliseconds (controls output rate)
    pub feature_step_ms: u32,

    /// Whether to perform artifact rejection
    pub artifact_rejection_enabled: bool,

    /// Amplitude threshold for artifact rejection (microvolts)
    pub artifact_threshold_uv: f32,
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            buffer_size_samples: 1024, // ~8 seconds at 128Hz
            notch_filter_enabled: true,
            notch_filter_hz: 60.0, // US default; should be 50.0 for EU
            bandpass_filter_enabled: true,
            bandpass_low_hz: 0.5,
            bandpass_high_hz: 45.0,
            feature_window_ms: 500,
            feature_step_ms: 50, // 20 Hz feature output
            artifact_rejection_enabled: true,
            artifact_threshold_uv: 100.0,
        }
    }
}

/// Configuration for the decoder (RL policy).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecoderConfig {
    /// Path to the decoder model file (relative to profile directory)
    pub model_path: String,

    /// Whether online learning is enabled
    pub online_learning_enabled: bool,

    /// Learning rate for online updates
    pub learning_rate: f32,

    /// Discount factor for RL
    pub gamma: f32,

    /// GAE lambda for PPO
    pub gae_lambda: f32,

    /// Number of steps between policy updates
    pub update_frequency_steps: u32,

    /// Batch size for updates
    pub batch_size: u32,

    /// Entropy coefficient for exploration
    pub entropy_coef: f32,

    /// Value function coefficient
    pub value_coef: f32,

    /// Maximum gradient norm for clipping
    pub max_grad_norm: f32,
}

impl Default for DecoderConfig {
    fn default() -> Self {
        Self {
            model_path: "decoder.pt".to_string(),
            online_learning_enabled: true,
            learning_rate: 3e-4,
            gamma: 0.99,
            gae_lambda: 0.95,
            update_frequency_steps: 128,
            batch_size: 32,
            entropy_coef: 0.01,
            value_coef: 0.5,
            max_grad_norm: 0.5,
        }
    }
}

/// Configuration for action output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionConfig {
    /// The action space definition
    pub action_space: ActionSpace,

    /// Mouse movement sensitivity multiplier
    pub mouse_sensitivity: f32,

    /// Whether to smooth mouse movements
    pub mouse_smoothing_enabled: bool,

    /// Mouse smoothing factor (0.0 = no smoothing, 1.0 = maximum smoothing)
    pub mouse_smoothing_factor: f32,

    /// Minimum confidence to execute an action
    pub min_confidence_threshold: f32,

    /// Minimum time between discrete actions (milliseconds)
    /// This prevents accidental double-taps
    pub action_debounce_ms: u32,

    /// Whether the system is currently enabled (can be toggled by user)
    pub enabled: bool,
}

impl Default for ActionConfig {
    fn default() -> Self {
        Self {
            action_space: ActionSpace::default(),
            mouse_sensitivity: 1.0,
            mouse_smoothing_enabled: true,
            mouse_smoothing_factor: 0.3,
            min_confidence_threshold: 0.5,
            action_debounce_ms: 100,
            enabled: true,
        }
    }
}

/// Configuration for storage and profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Base directory for all data storage
    /// Uses platform-specific default if not specified
    pub data_directory: Option<String>,

    /// Whether to encrypt sensitive data at rest
    pub encryption_enabled: bool,

    /// Whether to log session data for later analysis
    pub session_logging_enabled: bool,

    /// Maximum age of session logs before automatic deletion (days)
    pub session_log_retention_days: u32,

    /// Whether to periodically backup profiles
    pub auto_backup_enabled: bool,

    /// Interval between automatic backups (hours)
    pub backup_interval_hours: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_directory: None, // Will use platform default
            encryption_enabled: true,
            session_logging_enabled: true,
            session_log_retention_days: 30,
            auto_backup_enabled: true,
            backup_interval_hours: 24,
        }
    }
}

/// Configuration for the background service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Whether the service should start automatically on login
    pub auto_start: bool,

    /// Port for TCP localhost IPC (Python bridge communication)
    pub ipc_port: u16,

    /// Whether to show system tray icon
    pub show_tray_icon: bool,

    /// Whether to show notifications for important events
    pub notifications_enabled: bool,

    /// Log level (trace, debug, info, warn, error)
    pub log_level: String,

    /// Path to log file (None for stdout only)
    pub log_file_path: Option<String>,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            ipc_port: 47384,
            show_tray_icon: true,
            notifications_enabled: true,
            log_level: "info".to_string(),
            log_file_path: None,
        }
    }
}

/// Runtime state that's not persisted but useful for communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceState {
    /// Whether the service is currently running
    pub running: bool,

    /// Whether action output is currently enabled
    pub output_enabled: bool,

    /// Whether online learning is currently active
    pub learning_active: bool,

    /// Current active profile ID
    pub active_profile: Option<String>,

    /// Current device status summary
    pub device_status: String,

    /// Current error rate (recent window)
    pub recent_error_rate: f32,

    /// Uptime in seconds
    pub uptime_seconds: u64,
}

impl Default for ServiceState {
    fn default() -> Self {
        Self {
            running: false,
            output_enabled: false,
            learning_active: false,
            active_profile: None,
            device_status: "Disconnected".to_string(),
            recent_error_rate: 0.0,
            uptime_seconds: 0,
        }
    }
}
