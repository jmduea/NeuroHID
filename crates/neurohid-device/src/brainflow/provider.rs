//! # BrainFlow Device Provider
//!
//! Implements [`DeviceProvider`] for BrainFlow-supported devices.
//!
//! ## Discovery Model
//!
//! BrainFlow doesn't have a real device discovery mechanism — it relies on
//! the user knowing their board type and connection parameters (serial port,
//! IP address, etc.). We bridge this gap by:
//!
//! 1. Enumerating a configured list of board types to try
//! 2. Using BrainFlow's static metadata (channel count, sampling rate) to
//!    build `DeviceInfo` structs without needing a live connection
//! 3. Deferring actual hardware contact to `connect()`, which calls
//!    `prepare_session()` and will fail fast if the device isn't present
//!
//! The synthetic board is always "discoverable" and is useful for development.
//!
//! ## Connection Parameters
//!
//! Different BrainFlow boards need different `BrainFlowInputParams`:
//!
//! | Board             | Required Params                |
//! |-------------------|-------------------------------|
//! | SyntheticBoard    | (none)                        |
//! | CytonBoard        | serial_port                   |
//! | GanglionBoard     | serial_port                   |
//! | Muse2Board        | serial_port (via BLED112)      |
//! | UnicornBoard      | (auto-discovers via Bluetooth) |
//!
//! These are configured via [`BrainFlowConfig`].

use std::collections::HashMap;

use async_trait::async_trait;
use brainflow::board_shim::BoardShim;
use brainflow::brainflow_input_params::BrainFlowInputParamsBuilder;
use brainflow::BoardIds;

use neurohid_types::device::{
    ConnectionSettings, DeviceId, DeviceInfo, DeviceType,
};
use neurohid_types::error::{DeviceError, Result};

use crate::traits::{Device, DeviceProvider};

use super::board_map;
use super::device::BrainFlowDevice;
use super::stream::BoardChannelMap;

// ─── Configuration ───────────────────────────────────────────────────────────

/// Connection parameters for a specific BrainFlow board.
///
/// Wraps the board-specific fields that BrainFlow needs to establish
/// a session. Not all fields are used by all boards.
#[derive(Debug, Clone, Default)]
pub struct BoardParams {
    /// Serial port (e.g., "/dev/ttyUSB0", "COM3").
    /// Required for OpenBCI Cyton, Ganglion, and some others.
    pub serial_port: Option<String>,

    /// IP address for network-connected boards.
    pub ip_address: Option<String>,

    /// IP port for network-connected boards.
    pub ip_port: Option<i32>,

    /// MAC address for Bluetooth boards.
    pub mac_address: Option<String>,

    /// File path for playback boards.
    pub file: Option<String>,
}

/// Configuration for the BrainFlow provider.
#[derive(Debug, Clone)]
pub struct BrainFlowConfig {
    /// Which boards to expose during discovery.
    /// Default: the well-tested set from `board_map::supported_board_ids()`.
    pub board_ids: Vec<BoardIds>,

    /// Per-board connection parameters.
    /// Key is the board ID. Boards without an entry use default params.
    pub board_params: HashMap<i32, BoardParams>,

    /// Whether to always include the synthetic board in discovery
    /// (useful for development and testing).
    pub include_synthetic: bool,
}

impl Default for BrainFlowConfig {
    fn default() -> Self {
        Self {
            board_ids: board_map::supported_board_ids(),
            board_params: HashMap::new(),
            include_synthetic: true,
        }
    }
}

impl BrainFlowConfig {
    /// Create a config for development/testing with only the synthetic board.
    pub fn synthetic_only() -> Self {
        Self {
            board_ids: vec![BoardIds::SyntheticBoard],
            board_params: HashMap::new(),
            include_synthetic: true,
        }
    }

    /// Create a config for a specific board with connection params.
    pub fn for_board(board_id: BoardIds, params: BoardParams) -> Self {
        let mut board_params = HashMap::new();
        board_params.insert(board_id as i32, params);
        Self {
            board_ids: vec![board_id],
            board_params,
            include_synthetic: false,
        }
    }

    /// Set connection parameters for a specific board.
    pub fn with_params(mut self, board_id: BoardIds, params: BoardParams) -> Self {
        self.board_params.insert(board_id as i32, params);
        self
    }
}

// ─── Provider ────────────────────────────────────────────────────────────────

/// Device provider for all BrainFlow-supported boards.
///
/// A single provider instance can discover and connect to any of the configured
/// board types. The provider is stateless — all connection state lives in the
/// returned `BrainFlowDevice` instances.
pub struct BrainFlowDeviceProvider {
    config: BrainFlowConfig,
}

impl BrainFlowDeviceProvider {
    /// Create a new provider with the given configuration.
    pub fn new(config: BrainFlowConfig) -> Self {
        Self { config }
    }

    /// Create a provider with default settings (all supported boards + synthetic).
    pub fn with_defaults() -> Self {
        Self::new(BrainFlowConfig::default())
    }

    /// Build `BrainFlowInputParams` for a specific board from our config.
    fn build_input_params(&self, board_id: BoardIds) -> brainflow::brainflow_input_params::BrainFlowInputParams {
        let mut builder = BrainFlowInputParamsBuilder::default();

        if let Some(params) = self.config.board_params.get(&(board_id as i32)) {
            if let Some(ref port) = params.serial_port {
                builder = builder.serial_port(port.clone());
            }
            if let Some(ref ip) = params.ip_address {
                builder = builder.ip_address(ip.clone());
            }
            if let Some(port) = params.ip_port {
                builder = builder.ip_port(port);
            }
            if let Some(ref mac) = params.mac_address {
                builder = builder.mac_address(mac.clone());
            }
            if let Some(ref file) = params.file {
                builder = builder.file(file.clone());
            }
        }

        builder.build()
    }

    /// Generate a stable device ID for a board type.
    fn device_id_for(board_id: BoardIds) -> DeviceId {
        DeviceId::new(format!("brainflow_{:?}", board_id))
    }

    /// Build a `DeviceInfo` from BrainFlow static metadata (no live connection needed).
    fn build_device_info(&self, board_id: BoardIds) -> Option<DeviceInfo> {
        let channel_map = BoardChannelMap::for_board(board_id).ok()?;
        let channel_config = channel_map.to_channel_config();

        Some(DeviceInfo {
            id: Self::device_id_for(board_id),
            device_type: board_map::board_id_to_device_type(board_id),
            name: Some(board_map::board_display_name(board_id)),
            firmware_version: None,
            channel_config: Some(channel_config),
            battery_percent: None,
        })
    }

    /// Resolve a DeviceId back to a BoardIds.
    fn resolve_board_id(&self, device_id: &DeviceId) -> Option<BoardIds> {
        self.config.board_ids.iter().find(|&&bid| {
            Self::device_id_for(bid) == *device_id
        }).copied()
    }
}

#[async_trait]
impl DeviceProvider for BrainFlowDeviceProvider {
    fn device_type(&self) -> DeviceType {
        // The provider handles multiple device types, but we need to return one.
        // Return a generic type; individual devices report their specific type.
        DeviceType::Unknown("BrainFlow Multi-Device".into())
    }

    async fn is_available(&self) -> bool {
        // BrainFlow is a compiled-in dependency — if we're running, it's available.
        // The real check is whether specific boards can be connected to, which
        // happens in connect(). We just verify the library can be called.
        tokio::task::spawn_blocking(|| {
            // Try a trivial BrainFlow call to verify the native library loaded
            BoardShim::get_sampling_rate(BoardIds::SyntheticBoard as i32).is_ok()
        })
        .await
        .unwrap_or(false)
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>> {
        let board_ids = self.config.board_ids.clone();
        let config = self.config.clone();
        let include_synthetic = self.config.include_synthetic;

        // Build DeviceInfo for each configured board using static metadata.
        // This doesn't require hardware — just BrainFlow's built-in board specs.
        tokio::task::spawn_blocking(move || {
            let provider = BrainFlowDeviceProvider { config };
            let mut devices: Vec<DeviceInfo> = board_ids
                .iter()
                .filter_map(|&bid| provider.build_device_info(bid))
                .collect();

            // Ensure synthetic is present if requested and not already included
            if include_synthetic
                && !board_ids.contains(&BoardIds::SyntheticBoard)
            {
                if let Some(info) = provider.build_device_info(BoardIds::SyntheticBoard) {
                    devices.push(info);
                }
            }

            Ok(devices)
        })
        .await
        .map_err(|e| DeviceError::CommunicationError(format!("discovery task failed: {}", e)))?
    }

    async fn connect(
        &self,
        device_id: &DeviceId,
        settings: Option<ConnectionSettings>,
    ) -> Result<Box<dyn Device>> {
        let board_id = self.resolve_board_id(device_id)
            .ok_or(DeviceError::NoDeviceFound)?;

        let input_params = self.build_input_params(board_id);
        let channel_map = BoardChannelMap::for_board(board_id)?;

        // Create BoardShim and prepare session on a blocking thread
        // (prepare_session may involve hardware I/O: serial open, BT scan, etc.)
        let settings_clone = settings.clone();
        let board = tokio::task::spawn_blocking(move || -> Result<BoardShim> {
            let board = BoardShim::new(board_id, input_params)
                .map_err(|e| DeviceError::ConnectionFailed {
                    reason: format!("BoardShim::new failed for {:?}: {}", board_id, e),
                })?;

            board.prepare_session()
                .map_err(|e| DeviceError::ConnectionFailed {
                    reason: format!("prepare_session failed for {:?}: {}", board_id, e),
                })?;

            tracing::info!(board = ?board_id, "BrainFlow session prepared");
            Ok(board)
        })
        .await
        .map_err(|e| DeviceError::CommunicationError(format!("connect task failed: {}", e)))??;

        let device = BrainFlowDevice::new(board_id, board, channel_map, settings);

        Ok(Box::new(device))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require the BrainFlow native library to be installed.
    // They use the SyntheticBoard which doesn't need real hardware.
    // Run with: cargo test -p neurohid-device --features brainflow -- --ignored

    #[tokio::test]
    #[ignore = "requires BrainFlow native library"]
    async fn test_discover_includes_synthetic() {
        let provider = BrainFlowDeviceProvider::new(BrainFlowConfig::synthetic_only());
        let devices = provider.discover().await.unwrap();

        assert!(!devices.is_empty(), "should discover at least the synthetic board");

        let synthetic = devices.iter().find(|d| {
            matches!(d.device_type, DeviceType::Mock)
        });
        assert!(synthetic.is_some(), "synthetic board should be present");
    }

    #[tokio::test]
    #[ignore = "requires BrainFlow native library"]
    async fn test_connect_synthetic_board() {
        let provider = BrainFlowDeviceProvider::new(BrainFlowConfig::synthetic_only());
        let devices = provider.discover().await.unwrap();

        let device = provider.connect(&devices[0].id, None).await.unwrap();
        assert!(device.is_connected());
        assert!(!device.is_streaming());
    }

    #[tokio::test]
    #[ignore = "requires BrainFlow native library"]
    async fn test_connect_and_stream_synthetic() {
        use futures::StreamExt;

        let provider = BrainFlowDeviceProvider::new(BrainFlowConfig::synthetic_only());
        let devices = provider.discover().await.unwrap();

        let mut device = provider.connect(&devices[0].id, None).await.unwrap();
        let mut stream = device.start_streaming().await.unwrap();

        // Collect a few samples
        let mut count = 0;
        while count < 10 {
            if let Some(Ok(sample)) = stream.next().await {
                assert!(!sample.values.is_empty(), "sample should have channel data");
                count += 1;
            }
        }

        device.stop_streaming().await.unwrap();
        device.disconnect().await.unwrap();
        assert!(!device.is_connected());
    }

    #[tokio::test]
    #[ignore = "requires BrainFlow native library"]
    async fn test_is_available() {
        let provider = BrainFlowDeviceProvider::with_defaults();
        let available = provider.is_available().await;
        // This should be true if BrainFlow library is installed
        assert!(available, "BrainFlow should report as available");
    }

    #[tokio::test]
    #[ignore = "requires BrainFlow native library"]
    async fn test_connect_unknown_device_fails() {
        let provider = BrainFlowDeviceProvider::with_defaults();
        let bogus_id = DeviceId::new("brainflow_nonexistent");
        let result = provider.connect(&bogus_id, None).await;
        assert!(result.is_err());
    }
}
