//! # Configuration Types
//!
//! Types defining configuration for all NeuroHID components.
//! Configuration is hierarchical: there's a top-level system config
//! that contains configs for each subsystem.

use crate::{
    action::ActionSpace, device::ConnectionSettings, observation::ObservationConfig,
    observability::ObservabilityConfig, reward::ErrPConfig,
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

    /// Configuration for automatic recalibration prompts.
    #[serde(default)]
    pub recalibration: RecalibrationConfig,

    /// Configuration for the action output
    pub action: ActionConfig,

    /// Configuration for profile/storage
    pub storage: StorageConfig,

    /// Configuration for outbound streaming/outlet publishing.
    #[serde(default)]
    pub outlet: OutletConfig,

    /// Configuration for the service itself
    pub service: ServiceConfig,

    /// Configuration for hub UI behavior and persistence.
    #[serde(default)]
    pub ui: UiConfig,
}

/// Which device backend to use for data acquisition.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceBackend {
    /// Mock device for testing and development (no hardware needed).
    Mock,
    /// Lab Streaming Layer — consume any LSL stream on the local network.
    Lsl,
    /// Direct USB/serial adapter backend.
    Serial,
    /// Native BrainFlow backend.
    BrainFlow,
    /// Auto-detect: try LSL first, then fall back to Mock.
    #[default]
    Auto,
}

impl DeviceBackend {
    /// All variants in display order, for use in UI selectors.
    pub const ALL: &'static [DeviceBackend] = &[
        DeviceBackend::Auto,
        DeviceBackend::Lsl,
        DeviceBackend::Serial,
        DeviceBackend::BrainFlow,
        DeviceBackend::Mock,
    ];
}

impl std::fmt::Display for DeviceBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceBackend::Mock => write!(f, "Mock"),
            DeviceBackend::Lsl => write!(f, "LSL"),
            DeviceBackend::Serial => write!(f, "Serial"),
            DeviceBackend::BrainFlow => write!(f, "BrainFlow"),
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

/// Framing mode for serial sample decoding.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SerialFraming {
    /// Comma separated values, one sample per line.
    #[default]
    CsvLine,
    /// Raw bytes where each little-endian i16 word is a channel sample.
    BinaryI16Le,
}

/// Configuration for USB/serial adapter backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerialConfig {
    /// Explicit serial device path (e.g., `/dev/ttyUSB0`, `COM3`).
    #[serde(default)]
    pub port: Option<String>,
    /// Baud rate.
    #[serde(default = "default_serial_baud")]
    pub baud_rate: u32,
    /// Framing mode.
    #[serde(default)]
    pub framing: SerialFraming,
    /// Number of channels expected in each sample.
    #[serde(default = "default_serial_channels")]
    pub channels: usize,
}

fn default_serial_baud() -> u32 {
    115_200
}

fn default_serial_channels() -> usize {
    8
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port: None,
            baud_rate: default_serial_baud(),
            framing: SerialFraming::default(),
            channels: default_serial_channels(),
        }
    }
}

/// Configuration for native BrainFlow backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainFlowConfig {
    /// Board id understood by BrainFlow/OpenBCI.
    #[serde(default = "default_brainflow_board_id")]
    pub board_id: i32,
    /// Optional serial port for board connection.
    #[serde(default)]
    pub serial_port: Option<String>,
}

fn default_brainflow_board_id() -> i32 {
    0
}

impl Default for BrainFlowConfig {
    fn default() -> Self {
        Self {
            board_id: default_brainflow_board_id(),
            serial_port: None,
        }
    }
}

/// Configuration for device connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
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

    /// Serial-specific configuration (used when backend is Serial).
    #[serde(default)]
    pub serial: Option<SerialConfig>,

    /// BrainFlow-specific configuration (used when backend is BrainFlow).
    #[serde(default)]
    pub brainflow: Option<BrainFlowConfig>,
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

/// Transport options for outbound outlets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutletTransport {
    /// Publish newline-delimited JSON over TCP.
    #[default]
    TcpJson,
    /// Publish as LSL outlet stream(s).
    Lsl,
}

/// A single outlet target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutletTarget {
    /// Stable name used for display/debug.
    pub name: String,
    /// Transport kind.
    #[serde(default)]
    pub transport: OutletTransport,
    /// Transport address. TCP examples: `127.0.0.1:49000`.
    pub address: String,
    /// Whether this target is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for OutletTarget {
    fn default() -> Self {
        Self {
            name: "local-json".to_string(),
            transport: OutletTransport::default(),
            address: "127.0.0.1:49000".to_string(),
            enabled: true,
        }
    }
}

/// Configuration for configurable network outlets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutletConfig {
    /// Master switch for outlet publishing.
    #[serde(default)]
    pub enabled: bool,
    /// Destination targets.
    #[serde(default)]
    pub targets: Vec<OutletTarget>,
    /// Publish raw samples.
    #[serde(default)]
    pub publish_samples: bool,
    /// Publish extracted features.
    #[serde(default = "default_true")]
    pub publish_features: bool,
    /// Publish decoded actions.
    #[serde(default = "default_true")]
    pub publish_actions: bool,
    /// Publish markers/events.
    #[serde(default = "default_true")]
    pub publish_markers: bool,
}

fn default_true() -> bool {
    true
}

impl Default for OutletConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            targets: vec![OutletTarget::default()],
            publish_samples: false,
            publish_features: true,
            publish_actions: true,
            publish_markers: true,
        }
    }
}

/// Hub theme mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

/// Persisted UI preferences for the hub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Font scale multiplier (1.0 = default).
    #[serde(default = "default_font_scale")]
    pub font_scale: f32,
    /// End-user vs advanced UI mode.
    #[serde(default)]
    pub mode: UiMode,
    /// Theme preference.
    #[serde(default)]
    pub theme_mode: ThemeMode,
    /// Whether pane resizing is enabled in visualization layouts.
    #[serde(default = "default_true")]
    pub pane_resize_enabled: bool,
    /// Whether tray mode behavior is enabled.
    #[serde(default)]
    pub tray_mode_enabled: bool,
    /// Command used to bootstrap the managed Python environment for the IDE.
    #[serde(default = "default_jupyter_bootstrap_command")]
    pub jupyter_bootstrap_command: String,
    /// Whether the IDE should bootstrap dependencies automatically.
    #[serde(default = "default_true")]
    pub jupyter_auto_bootstrap: bool,
    /// Command used by Advanced mode to launch JupyterLab.
    #[serde(default = "default_jupyter_command")]
    pub jupyter_command: String,
    /// URL opened by the IDE when Jupyter server is ready.
    #[serde(default = "default_jupyter_url")]
    pub jupyter_url: String,
    /// Persisted visualization layout preset key.
    #[serde(default = "default_visualization_layout_preset")]
    pub visualization_layout_preset: String,
    /// Persisted visualization widget assignments by pane slot.
    #[serde(default = "default_visualization_pane_widgets")]
    pub visualization_pane_widgets: Vec<String>,
    /// Serialized `egui_tiles` tree state for visualization layout.
    #[serde(default)]
    pub visualization_layout_tree_json: Option<String>,
    /// Command used by Python Lab to launch a notebook-compatible kernel adapter.
    ///
    /// Deprecated: retained for backward compatibility with older configs.
    #[serde(default = "default_lab_kernel_command")]
    pub lab_kernel_command: String,
}

fn default_font_scale() -> f32 {
    1.0
}

fn default_lab_kernel_command() -> String {
    "uv run --directory python neurohid-ml lab-kernel --stdio".to_string()
}

fn default_jupyter_bootstrap_command() -> String {
    "uv sync --directory python".to_string()
}

fn default_jupyter_command() -> String {
    "uv run --directory python jupyter lab --no-browser --ip=127.0.0.1 --port=8888".to_string()
}

fn default_jupyter_url() -> String {
    "http://127.0.0.1:8888/lab".to_string()
}

fn default_visualization_layout_preset() -> String {
    "grid2x2".to_string()
}

fn default_visualization_pane_widgets() -> Vec<String> {
    vec![
        "time_series".to_string(),
        "fft_plot".to_string(),
        "band_power".to_string(),
        "signal_quality".to_string(),
    ]
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            font_scale: default_font_scale(),
            mode: UiMode::default(),
            theme_mode: ThemeMode::default(),
            pane_resize_enabled: true,
            tray_mode_enabled: false,
            jupyter_bootstrap_command: default_jupyter_bootstrap_command(),
            jupyter_auto_bootstrap: default_true(),
            jupyter_command: default_jupyter_command(),
            jupyter_url: default_jupyter_url(),
            visualization_layout_preset: default_visualization_layout_preset(),
            visualization_pane_widgets: default_visualization_pane_widgets(),
            visualization_layout_tree_json: None,
            lab_kernel_command: default_lab_kernel_command(),
        }
    }
}

/// Hub UI mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiMode {
    /// Default mode for daily use.
    #[default]
    Standard,
    /// Power-user/research mode with advanced tooling.
    Advanced,
}

/// Runtime hosting mode for the service control surface.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceRuntimeMode {
    /// Hub owns and runs the runtime process in-process.
    #[default]
    Embedded,
    /// Hub connects to an already-running external `neurohid-service`.
    External,
}

impl ServiceRuntimeMode {
    /// All runtime mode variants in display order.
    pub const ALL: &'static [ServiceRuntimeMode] =
        &[ServiceRuntimeMode::Embedded, ServiceRuntimeMode::External];
}

impl std::fmt::Display for ServiceRuntimeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceRuntimeMode::Embedded => write!(f, "Embedded"),
            ServiceRuntimeMode::External => write!(f, "External"),
        }
    }
}

/// Transport for hub/runtime control protocol.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlTransport {
    /// Windows named pipe transport.
    #[default]
    NamedPipe,
    /// Localhost TCP fallback for non-Windows development.
    TcpLoopback,
}

/// Transport for runtime<->trainer ML bridge.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MlTransport {
    /// Windows named pipe transport.
    #[default]
    NamedPipe,
    /// Localhost TCP fallback for non-Windows development.
    TcpLoopback,
}

/// Strategy used when the primary deep model path is unavailable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackModelStrategy {
    /// Use a lightweight Rust model for degraded/fallback operation.
    #[default]
    LightweightRust,
}

/// Runtime capability gating policy in fallback mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FallbackPolicy {
    /// Master switch for fallback behavior.
    pub enabled: bool,
    /// Model strategy used for fallback inference.
    pub model_strategy: FallbackModelStrategy,
    /// Rolling evaluation window for capability confidence/success scores.
    pub gate_window_secs: u64,
    /// Movement gating minimums.
    pub movement_min_confidence: f32,
    pub movement_min_success_score: f32,
    /// Click gating minimums.
    pub click_min_confidence: f32,
    pub click_min_success_score: f32,
    /// Keyboard gating minimums.
    pub keyboard_min_confidence: f32,
    pub keyboard_min_success_score: f32,
    /// Hold time before re-enabling a previously disabled capability.
    pub capability_reenable_hold_secs: u64,
    /// Cooldown between repeated fallback/degraded notifications.
    pub notification_cooldown_secs: u64,
}

impl Default for FallbackPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            model_strategy: FallbackModelStrategy::default(),
            gate_window_secs: 60,
            movement_min_confidence: 0.65,
            movement_min_success_score: 0.70,
            click_min_confidence: 0.80,
            click_min_success_score: 0.80,
            keyboard_min_confidence: 0.85,
            keyboard_min_success_score: 0.85,
            capability_reenable_hold_secs: 15,
            notification_cooldown_secs: 120,
        }
    }
}

/// Configuration for automatic recalibration prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecalibrationConfig {
    /// Minimum rolling signal quality before prompting recalibration.
    pub rolling_signal_quality_threshold: f32,
    /// Maximum rolling error rate before prompting recalibration.
    pub rolling_error_rate_threshold: f32,
    /// Duration threshold conditions must persist before prompting.
    pub sustained_duration_secs: u64,
    /// Minimum cooldown between recalibration prompts.
    pub notification_cooldown_secs: u64,
}

impl Default for RecalibrationConfig {
    fn default() -> Self {
        Self {
            rolling_signal_quality_threshold: 0.5,
            rolling_error_rate_threshold: 0.35,
            sustained_duration_secs: 120,
            notification_cooldown_secs: 900,
        }
    }
}

/// Configuration for the background service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// Runtime hosting mode for hub control.
    #[serde(default)]
    pub runtime_mode: ServiceRuntimeMode,

    /// Host used for runtime control protocol connections in external mode.
    #[serde(default = "default_control_host")]
    pub control_host: String,

    /// Port used for runtime control protocol connections in external mode.
    #[serde(default = "default_control_port")]
    pub control_port: u16,

    /// Transport used for runtime control protocol.
    #[serde(default)]
    pub control_transport: ControlTransport,

    /// Named pipe used for runtime control protocol.
    #[serde(default = "default_control_pipe_name")]
    pub control_pipe_name: String,

    /// Whether the service should start automatically on login
    pub auto_start: bool,

    /// Port for TCP localhost IPC (Python bridge communication)
    pub ipc_port: u16,

    /// Transport used for runtime ML bridge communications.
    #[serde(default)]
    pub ml_transport: MlTransport,

    /// Named pipe used for runtime ML bridge communications.
    #[serde(default = "default_ml_pipe_name")]
    pub ml_pipe_name: String,

    /// Whether to use the built-in simulated IPC bridge when no real
    /// Python process bridge is configured.
    #[serde(default = "default_ipc_simulation_enabled")]
    pub ipc_simulation_enabled: bool,

    /// Maximum trainer heartbeat staleness before bridge is marked stalled.
    #[serde(default = "default_ml_stall_timeout_ms")]
    pub ml_stall_timeout_ms: u64,

    /// Expected heartbeat interval for runtime<->trainer bridge.
    #[serde(default = "default_ml_heartbeat_interval_ms")]
    pub ml_heartbeat_interval_ms: u64,

    /// Whether to show system tray icon
    pub show_tray_icon: bool,

    /// Whether to show notifications for important events
    pub notifications_enabled: bool,

    /// Log level (trace, debug, info, warn, error)
    pub log_level: String,

    /// Path to log file (None for stdout only)
    pub log_file_path: Option<String>,

    /// Latency alert policy for runtime decode/action p95 metrics.
    #[serde(default)]
    pub latency_alert: LatencyAlertConfig,

    /// Capability gating and model fallback policy.
    #[serde(default)]
    pub fallback_policy: FallbackPolicy,

    /// Runtime observability sampling and rate-limit policy.
    #[serde(default)]
    pub observability: ObservabilityConfig,
}

fn default_ipc_simulation_enabled() -> bool {
    true
}

fn default_control_host() -> String {
    "127.0.0.1".to_string()
}

fn default_control_port() -> u16 {
    47_385
}

fn default_control_pipe_name() -> String {
    r"\\.\pipe\neurohid.control.v2".to_string()
}

fn default_ml_pipe_name() -> String {
    r"\\.\pipe\neurohid.ml.v2".to_string()
}

fn default_ml_stall_timeout_ms() -> u64 {
    1_500
}

fn default_ml_heartbeat_interval_ms() -> u64 {
    500
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            runtime_mode: ServiceRuntimeMode::default(),
            control_host: default_control_host(),
            control_port: default_control_port(),
            control_transport: ControlTransport::default(),
            control_pipe_name: default_control_pipe_name(),
            auto_start: false,
            ipc_port: 47384,
            ml_transport: MlTransport::default(),
            ml_pipe_name: default_ml_pipe_name(),
            ipc_simulation_enabled: true,
            ml_stall_timeout_ms: default_ml_stall_timeout_ms(),
            ml_heartbeat_interval_ms: default_ml_heartbeat_interval_ms(),
            show_tray_icon: true,
            notifications_enabled: true,
            log_level: "info".to_string(),
            log_file_path: None,
            latency_alert: LatencyAlertConfig::default(),
            fallback_policy: FallbackPolicy::default(),
            observability: ObservabilityConfig::default(),
        }
    }
}

/// Runtime latency alert thresholds and notification policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyAlertConfig {
    /// Master switch for latency alert monitoring.
    pub enabled: bool,
    /// Decoder latency p95 threshold in microseconds.
    pub decode_p95_threshold_us: u64,
    /// End-to-end action latency p95 threshold in microseconds.
    pub action_p95_threshold_us: u64,
    /// Duration thresholds must remain exceeded before alert activates.
    pub sustained_duration_secs: u64,
    /// Cooldown between repeated warning notifications.
    pub notification_cooldown_secs: u64,
}

impl Default for LatencyAlertConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            decode_p95_threshold_us: 35_000,
            action_p95_threshold_us: 60_000,
            sustained_duration_secs: 30,
            notification_cooldown_secs: 120,
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

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::ServiceConfig;

    #[test]
    fn service_config_defaults_include_observability_policy() {
        let config = ServiceConfig::default();
        assert!(config.observability.global.sample_ratio > 0.0);
        assert!(config.observability.signal.debug_max_per_second > 0);
        assert!(config.observability.control.info_max_per_minute > 0);
    }

    #[test]
    fn service_config_backcompat_deserialize_without_observability_field() {
        let mut json =
            serde_json::to_value(ServiceConfig::default()).expect("serialize default service");
        let object = json
            .as_object_mut()
            .expect("service config should serialize as object");
        object.remove("observability");

        let decoded: ServiceConfig =
            serde_json::from_value(json).expect("deserialize service config without observability");
        assert!(decoded.observability.decoder.sample_ratio > 0.0);
        assert!(decoded.observability.ipc.debug_max_per_second > 0);
    }

    #[test]
    fn service_config_roundtrip_preserves_observability_field_shape() {
        let config = ServiceConfig::default();
        let json = serde_json::to_value(&config).expect("serialize service config");
        let observability = json
            .get("observability")
            .and_then(Value::as_object)
            .expect("observability object exists");
        assert!(observability.contains_key("global"));
        assert!(observability.contains_key("signal"));
        assert!(observability.contains_key("decoder"));
        assert!(observability.contains_key("action"));
        assert!(observability.contains_key("ipc"));
        assert!(observability.contains_key("control"));
    }
}
