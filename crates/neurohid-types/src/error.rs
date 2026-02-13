//! # Error Types
//!
//! Unified error types for the NeuroHID project. We use `thiserror` for
//! clean error definitions and easy conversion between error types.
//!
//! ## Error Philosophy
//!
//! Errors are categorized by the subsystem they originate from. Each subsystem
//! has its own error enum, and there's a top-level `Error` enum that wraps them all.
//! This allows fine-grained error handling when needed, while also supporting
//! simple `?` propagation.

use thiserror::Error;

/// A convenient Result type alias using our Error type.
pub type Result<T> = std::result::Result<T, Error>;

/// The top-level error type that encompasses all NeuroHID errors.
#[derive(Error, Debug)]
pub enum Error {
    /// Errors related to device connection and communication
    #[error("Device error: {0}")]
    Device(#[from] DeviceError),

    /// Errors related to signal processing
    #[error("Signal processing error: {0}")]
    Signal(#[from] SignalError),

    /// Errors related to ErrP detection
    #[error("ErrP detection error: {0}")]
    ErrP(#[from] ErrPError),

    /// Errors related to the decoder
    #[error("Decoder error: {0}")]
    Decoder(#[from] DecoderError),

    /// Errors related to platform/HID operations
    #[error("Platform error: {0}")]
    Platform(#[from] PlatformError),

    /// Errors related to storage and profiles
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    /// Errors related to IPC communication
    #[error("IPC error: {0}")]
    Ipc(#[from] IpcError),

    /// Errors related to calibration
    #[error("Calibration error: {0}")]
    Calibration(#[from] CalibrationError),

    /// Errors related to configuration
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Generic internal error (should be rare)
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Errors related to device connection and communication.
#[derive(Error, Debug)]
pub enum DeviceError {
    /// No device found
    #[error("No compatible device found")]
    NoDeviceFound,

    /// Device not connected
    #[error("Device not connected")]
    NotConnected,

    /// Connection failed
    #[error("Failed to connect to device: {reason}")]
    ConnectionFailed { reason: String },

    /// Connection lost
    #[error("Connection to device lost: {reason}")]
    ConnectionLost { reason: String },

    /// Device not supported
    #[error("Device type not supported: {device_type}")]
    UnsupportedDevice { device_type: String },

    /// Communication error
    #[error("Device communication error: {0}")]
    CommunicationError(String),

    /// Device returned invalid data
    #[error("Invalid data from device: {0}")]
    InvalidData(String),

    /// Timeout waiting for device
    #[error("Device operation timed out")]
    Timeout,

    /// Device busy (e.g., in use by another application)
    #[error("Device is busy or in use by another application")]
    DeviceBusy,

    /// Permission denied
    #[error("Permission denied to access device: {0}")]
    PermissionDenied(String),

    /// Cortex API specific error
    #[error("Cortex API error: {code} - {message}")]
    CortexApiError { code: i32, message: String },
}

/// Errors related to signal processing.
#[derive(Error, Debug)]
pub enum SignalError {
    /// Buffer overflow (data coming faster than we can process)
    #[error("Signal buffer overflow")]
    BufferOverflow,

    /// Buffer underflow (not enough data to process)
    #[error("Not enough data in buffer: have {available}, need {required}")]
    BufferUnderflow { available: usize, required: usize },

    /// Invalid channel configuration
    #[error("Invalid channel configuration: {0}")]
    InvalidChannelConfig(String),

    /// Feature extraction failed
    #[error("Feature extraction failed: {0}")]
    FeatureExtractionFailed(String),

    /// Filter configuration error
    #[error("Invalid filter configuration: {0}")]
    InvalidFilterConfig(String),

    /// Signal quality too poor
    #[error("Signal quality too poor for processing")]
    PoorSignalQuality,

    /// Numeric computation error (NaN, Inf, etc.)
    #[error("Numeric error during signal processing: {0}")]
    NumericError(String),
}

/// Errors related to ErrP detection.
#[derive(Error, Debug)]
pub enum ErrPError {
    /// Not calibrated
    #[error("ErrP detector not calibrated")]
    NotCalibrated,

    /// Classifier not loaded
    #[error("ErrP classifier not loaded")]
    ClassifierNotLoaded,

    /// Detection failed
    #[error("ErrP detection failed: {0}")]
    DetectionFailed(String),

    /// Invalid window
    #[error("Invalid ErrP window: {0}")]
    InvalidWindow(String),

    /// Model file error
    #[error("Error loading ErrP model: {0}")]
    ModelLoadError(String),
}

/// Errors related to the decoder (RL policy).
#[derive(Error, Debug)]
pub enum DecoderError {
    /// Model not loaded
    #[error("Decoder model not loaded")]
    ModelNotLoaded,

    /// Invalid input dimensions
    #[error("Invalid input dimensions: expected {expected}, got {got}")]
    InvalidInputDimensions { expected: usize, got: usize },

    /// Inference failed
    #[error("Decoder inference failed: {0}")]
    InferenceFailed(String),

    /// Training failed
    #[error("Decoder training failed: {0}")]
    TrainingFailed(String),

    /// Model file error
    #[error("Error with decoder model file: {0}")]
    ModelFileError(String),

    /// IPC communication error with Python
    #[error("Failed to communicate with Python ML process: {0}")]
    PythonBridgeError(String),
}

/// Errors related to platform/HID operations.
#[derive(Error, Debug)]
pub enum PlatformError {
    /// HID emulation not available
    #[error("HID emulation not available on this platform")]
    HidNotAvailable,

    /// Failed to emit input event
    #[error("Failed to emit input event: {0}")]
    InputEmissionFailed(String),

    /// Failed to get cursor position
    #[error("Failed to get cursor position: {0}")]
    CursorQueryFailed(String),

    /// Failed to get screen info
    #[error("Failed to get screen information: {0}")]
    ScreenQueryFailed(String),

    /// Permission denied for input simulation
    #[error("Permission denied for input simulation. {hint}")]
    PermissionDenied { hint: String },

    /// Platform-specific feature not supported
    #[error("Feature not supported on this platform: {0}")]
    NotSupported(String),

    /// Accessibility API error
    #[error("Accessibility API error: {0}")]
    AccessibilityError(String),
}

/// Errors related to storage and profiles.
#[derive(Error, Debug)]
pub enum StorageError {
    /// Profile not found
    #[error("Profile not found: {profile_id}")]
    ProfileNotFound { profile_id: String },

    /// Profile already exists
    #[error("Profile already exists: {profile_id}")]
    ProfileAlreadyExists { profile_id: String },

    /// Failed to read file
    #[error("Failed to read file '{path}': {reason}")]
    ReadError { path: String, reason: String },

    /// Failed to write file
    #[error("Failed to write file '{path}': {reason}")]
    WriteError { path: String, reason: String },

    /// Encryption/decryption error
    #[error("Encryption error: {0}")]
    EncryptionError(String),

    /// Keyring error
    #[error("Keyring error: {0}")]
    KeyringError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Data corruption
    #[error("Data corruption detected in {location}: {details}")]
    DataCorruption { location: String, details: String },

    /// Storage full
    #[error("Storage quota exceeded")]
    StorageFull,
}

/// Errors related to IPC communication.
#[derive(Error, Debug)]
pub enum IpcError {
    /// Connection failed
    #[error("IPC connection failed: {0}")]
    ConnectionFailed(String),

    /// Connection lost
    #[error("IPC connection lost")]
    ConnectionLost,

    /// Message send failed
    #[error("Failed to send IPC message: {0}")]
    SendFailed(String),

    /// Message receive failed
    #[error("Failed to receive IPC message: {0}")]
    ReceiveFailed(String),

    /// Invalid message format
    #[error("Invalid IPC message format: {0}")]
    InvalidMessage(String),

    /// Timeout
    #[error("IPC operation timed out")]
    Timeout,

    /// Python process not running
    #[error("Python ML process not running")]
    PythonProcessNotRunning,
}

/// Errors related to calibration.
#[derive(Error, Debug)]
pub enum CalibrationError {
    /// Calibration already in progress
    #[error("Calibration already in progress")]
    AlreadyInProgress,

    /// Calibration not in progress
    #[error("No calibration in progress")]
    NotInProgress,

    /// Insufficient data
    #[error("Insufficient calibration data: {reason}")]
    InsufficientData { reason: String },

    /// Calibration failed
    #[error("Calibration failed: {0}")]
    Failed(String),

    /// Signal quality too poor for calibration
    #[error("Signal quality too poor for calibration")]
    PoorSignalQuality,

    /// User cancelled
    #[error("Calibration cancelled by user")]
    Cancelled,

    /// Session data error
    #[error("Calibration session data error: {0}")]
    SessionDataError(String),
}

/// Errors related to configuration.
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Config file not found
    #[error("Configuration file not found: {path}")]
    FileNotFound { path: String },

    /// Config parsing error
    #[error("Failed to parse configuration: {0}")]
    ParseError(String),

    /// Invalid configuration value
    #[error("Invalid configuration value for '{key}': {reason}")]
    InvalidValue { key: String, reason: String },

    /// Missing required configuration
    #[error("Missing required configuration: {key}")]
    MissingRequired { key: String },

    /// Config write error
    #[error("Failed to write configuration: {0}")]
    WriteError(String),
}

// Implement From for common std error types to make ? work smoothly

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Storage(StorageError::ReadError {
            path: "unknown".to_string(),
            reason: err.to_string(),
        })
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Storage(StorageError::SerializationError(err.to_string()))
    }
}

// Helper functions for creating errors with context

impl Error {
    /// Create an internal error with a message
    pub fn internal(msg: impl Into<String>) -> Self {
        Error::Internal(msg.into())
    }
}

impl DeviceError {
    /// Create a connection failed error with reason
    pub fn connection_failed(reason: impl Into<String>) -> Self {
        DeviceError::ConnectionFailed {
            reason: reason.into(),
        }
    }
}

impl StorageError {
    /// Create a read error with path and reason
    pub fn read_error(path: impl Into<String>, reason: impl Into<String>) -> Self {
        StorageError::ReadError {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Create a write error with path and reason
    pub fn write_error(path: impl Into<String>, reason: impl Into<String>) -> Self {
        StorageError::WriteError {
            path: path.into(),
            reason: reason.into(),
        }
    }
}
