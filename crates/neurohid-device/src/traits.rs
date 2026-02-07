//! # Device Traits
//!
//! This module defines the core abstractions for biosensor devices.
//! The goal is to provide a uniform interface that works across different
//! hardware (Emotiv, OpenBCI, Muse, etc.) while allowing device-specific
//! features when needed.
//!
//! ## Architecture Overview
//!
//! The device layer has three main responsibilities:
//!
//! 1. **Discovery**: Finding available devices
//! 2. **Connection**: Establishing and maintaining device connections
//! 3. **Streaming**: Providing a continuous stream of samples
//!
//! Each device implementation provides these capabilities through the traits
//! defined here. The traits use `async_trait` because all I/O operations
//! (Bluetooth, USB, WebSocket) are inherently asynchronous.
//!
//! ## Example Usage
//!
//! ```ignore
//! // Create a device provider (implementation-specific)
//! let provider = EmotivProvider::new(config).await?;
//!
//! // Discover available devices
//! let devices = provider.discover().await?;
//!
//! // Connect to the first device
//! let mut device = provider.connect(&devices[0].id).await?;
//!
//! // Start streaming
//! let mut stream = device.start_streaming().await?;
//!
//! // Process samples
//! while let Some(sample) = stream.next().await {
//!     process_sample(sample?);
//! }
//! ```

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use neurohid_types::{
    device::{ConnectionSettings, DeviceId, DeviceInfo, DeviceStatus, DeviceType},
    error::Result,
    signal::{DeviceChannelConfig, Sample},
};

/// A stream of samples from a device.
/// This is a type alias for a pinned, boxed, async stream of Results containing Samples.
///
/// We use this type alias because Rust's async streams are complex to express,
/// and this gives us a consistent interface across all device implementations.
pub type SampleStream = Pin<Box<dyn Stream<Item = Result<Sample>> + Send>>;

/// Provides discovery and connection capabilities for a specific type of device.
///
/// Each device family (Emotiv, OpenBCI, etc.) has its own implementation of this trait.
/// The provider is responsible for finding devices and creating connections to them.
///
/// ## Lifecycle
///
/// 1. Create a provider with device-family-specific configuration
/// 2. Call `discover()` to find available devices
/// 3. Call `connect()` with a device ID to establish a connection
/// 4. The returned `Device` can then be used for streaming
///
/// ## Thread Safety
///
/// Providers should be `Send + Sync` to allow sharing across threads.
/// Connection state is managed per-device, not per-provider.
#[async_trait]
pub trait DeviceProvider: Send + Sync {
    /// Returns the type of devices this provider handles.
    /// This is used for logging and user feedback.
    fn device_type(&self) -> DeviceType;

    /// Returns whether this provider is available on the current system.
    ///
    /// This might check for required system services (e.g., Bluetooth availability),
    /// required software (e.g., Emotiv Cortex service), or platform support.
    ///
    /// If this returns `false`, calling other methods will likely fail.
    async fn is_available(&self) -> bool;

    /// Discovers available devices.
    ///
    /// This scans for devices that can potentially be connected to. The scan
    /// duration and method are implementation-specific (Bluetooth scan,
    /// USB enumeration, service query, etc.).
    ///
    /// # Returns
    ///
    /// A vector of `DeviceInfo` for each discovered device. The vector may be
    /// empty if no devices are found. Previously discovered devices that are
    /// no longer available may not be included.
    ///
    /// # Errors
    ///
    /// Returns an error if the discovery process fails (e.g., Bluetooth not
    /// available, permissions denied).
    async fn discover(&self) -> Result<Vec<DeviceInfo>>;

    /// Connects to a specific device.
    ///
    /// Establishes a connection to the device with the given ID. The device
    /// must have been previously discovered (the ID comes from `DeviceInfo`).
    ///
    /// # Arguments
    ///
    /// * `device_id` - The unique identifier of the device to connect to
    /// * `settings` - Optional connection settings; uses defaults if None
    ///
    /// # Returns
    ///
    /// A boxed `Device` trait object that can be used to stream data.
    /// The device is connected but not yet streaming when returned.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The device is not found or no longer available
    /// - Connection fails (timeout, refused, etc.)
    /// - The device is already connected elsewhere
    async fn connect(
        &self,
        device_id: &DeviceId,
        settings: Option<ConnectionSettings>,
    ) -> Result<Box<dyn Device>>;
}

/// Represents a connected biosensor device.
///
/// This trait provides the interface for interacting with a device after
/// connection has been established. The main operations are:
///
/// - Getting device information and status
/// - Starting/stopping data streaming
/// - Monitoring connection health
///
/// ## Streaming Model
///
/// Data streaming follows a pull-based model using Rust's async Stream trait.
/// When you call `start_streaming()`, you get back a stream that yields samples
/// as they become available. The stream ends when:
///
/// - You call `stop_streaming()`
/// - The connection is lost
/// - An unrecoverable error occurs
///
/// ## Connection Management
///
/// The device monitors its own connection health. You can subscribe to status
/// updates to react to connection issues. If auto-reconnect is enabled in the
/// settings, the device will attempt to reconnect automatically.
#[async_trait]
pub trait Device: Send + Sync {
    /// Returns the unique identifier for this device.
    fn id(&self) -> &DeviceId;

    /// Returns information about this device.
    fn info(&self) -> &DeviceInfo;

    /// Returns the channel configuration for this device.
    ///
    /// This includes the number of channels, their positions, sampling rate,
    /// and other signal characteristics.
    fn channel_config(&self) -> &DeviceChannelConfig;

    /// Returns the current status of the device.
    ///
    /// This includes connection state, streaming state, sample counts,
    /// battery level, and signal quality metrics.
    fn status(&self) -> DeviceStatus;

    /// Checks if the device is currently connected.
    fn is_connected(&self) -> bool;

    /// Checks if the device is currently streaming data.
    fn is_streaming(&self) -> bool;

    /// Starts streaming data from the device.
    ///
    /// This begins the data acquisition process. Samples will be available
    /// from the returned stream as they arrive from the device.
    ///
    /// # Returns
    ///
    /// A stream of samples. Each item is a `Result<Sample>` to allow for
    /// per-sample error handling (e.g., corrupted packets).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The device is not connected
    /// - Streaming is already in progress
    /// - The device fails to start acquisition
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut stream = device.start_streaming().await?;
    /// while let Some(result) = stream.next().await {
    ///     match result {
    ///         Ok(sample) => process_sample(sample),
    ///         Err(e) => handle_sample_error(e),
    ///     }
    /// }
    /// ```
    async fn start_streaming(&mut self) -> Result<SampleStream>;

    /// Stops streaming data from the device.
    ///
    /// This halts data acquisition. The stream returned from `start_streaming()`
    /// will end after this is called. It's safe to call this even if not
    /// currently streaming (it will be a no-op).
    ///
    /// # Errors
    ///
    /// Returns an error if the stop command fails to be sent to the device.
    /// The stream may have already stopped due to a connection issue.
    async fn stop_streaming(&mut self) -> Result<()>;

    /// Disconnects from the device.
    ///
    /// This cleanly terminates the connection. If streaming is in progress,
    /// it will be stopped first. After disconnection, the device object
    /// should not be used further.
    ///
    /// # Errors
    ///
    /// Returns an error if the disconnection process fails. Even if an error
    /// is returned, the device should be considered disconnected.
    async fn disconnect(&mut self) -> Result<()>;

    /// Returns a stream of status updates.
    ///
    /// This allows monitoring the device's connection health, battery level,
    /// and signal quality over time. Status updates are pushed when there's
    /// a significant change (not on every sample).
    ///
    /// # Returns
    ///
    /// A stream of `DeviceStatus` values. The stream ends when the device
    /// is disconnected.
    fn status_stream(&self) -> Pin<Box<dyn Stream<Item = DeviceStatus> + Send>>;
}

/// Extension trait providing convenience methods for devices.
///
/// These are default implementations that work with any `Device`, providing
/// higher-level operations built on the core trait methods.
pub trait DeviceExt: Device {
    /// Waits for the device to reach a minimum signal quality.
    ///
    /// This is useful during setup to ensure signal quality is acceptable
    /// before starting calibration or use.
    ///
    /// # Arguments
    ///
    /// * `min_quality` - Minimum average quality across channels (0.0 to 1.0)
    /// * `timeout_ms` - Maximum time to wait in milliseconds
    ///
    /// # Returns
    ///
    /// `true` if the quality threshold was reached, `false` if timeout.
    fn wait_for_quality(
        &self,
        min_quality: f32,
        timeout_ms: u64,
    ) -> impl std::future::Future<Output = bool> + Send;
}

