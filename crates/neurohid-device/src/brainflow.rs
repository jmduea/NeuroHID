//! BrainFlow simulation adapter with normalized board metadata.
//!
//! This adapter wraps a mock device behind the BrainFlow board catalogue so
//! that board metadata (channels, sampling rates, resolution) matches
//! real BrainFlow boards. No actual BrainFlow SDK is linked — the adapter
//! is intended for offline development, UI testing, and CI.
//!
//! When the native BrainFlow SDK integration is needed, add the `brainflow`
//! feature and plumb the real SDK behind this same `DeviceProvider` trait.
//!
//! With `brainflow-native` enabled, connect() returns a native Device for real
//! boards (board_id != 0 or serial_port set); board_id 0 stays synthetic.

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use neurohid_types::{
    config::BrainFlowConfig,
    device::{ConnectionSettings, DeviceId, DeviceInfo, DeviceStatus, DeviceType},
    error::{DeviceError, Result},
    signal::{ChannelConfig, ChannelId, DeviceChannelConfig},
};

use crate::{
    mock::{MockDevice, MockDeviceConfig},
    traits::{Device, DeviceProvider, SampleStream},
};

const FALLBACK_CHANNEL_COUNT: usize = 8;
const FALLBACK_SAMPLING_RATE_HZ: f32 = 250.0;
const FALLBACK_RESOLUTION_BITS: u8 = 24;

/// Static descriptor for a well-known board used in synthetic (no-hardware) mode.
struct KnownBoard {
    board_id: i32,
    display_name: &'static str,
    channel_count: usize,
    sampling_rate_hz: f32,
    resolution_bits: u8,
    /// Electrode names in 10-20 order (length must equal `channel_count`).
    channel_names: &'static [&'static str],
}

/// Pre-defined board descriptors surfaced during synthetic discovery.
///
/// When [`BrainFlowProvider`] is in synthetic mode (board_id 0, no serial port),
/// `discover()` returns one [`DeviceInfo`] per entry so the caller can choose
/// which hardware profile to simulate.
const KNOWN_SYNTHETIC_BOARDS: &[KnownBoard] = &[
    KnownBoard {
        board_id: 0,
        display_name: "BrainFlow Synthetic Board",
        channel_count: 8,
        sampling_rate_hz: 250.0,
        resolution_bits: 24,
        channel_names: &["Fp1", "Fp2", "C3", "C4", "P7", "P8", "O1", "O2"],
    },
    KnownBoard {
        board_id: 1,
        display_name: "OpenBCI Cyton (Synthetic)",
        channel_count: 8,
        sampling_rate_hz: 250.0,
        resolution_bits: 24,
        channel_names: &["Fp1", "Fp2", "C3", "C4", "P7", "P8", "O1", "O2"],
    },
    KnownBoard {
        board_id: 2,
        display_name: "OpenBCI Ganglion (Synthetic)",
        channel_count: 4,
        sampling_rate_hz: 200.0,
        resolution_bits: 24,
        channel_names: &["Fp1", "Fp2", "C3", "C4"],
    },
    KnownBoard {
        board_id: 3,
        display_name: "Emotiv Insight (Synthetic)",
        channel_count: 5,
        sampling_rate_hz: 128.0,
        resolution_bits: 14,
        channel_names: &["AF3", "AF4", "T7", "T8", "Pz"],
    },
    KnownBoard {
        board_id: 4,
        display_name: "Muse 2 (Synthetic)",
        channel_count: 5,
        sampling_rate_hz: 256.0,
        resolution_bits: 12,
        channel_names: &["TP9", "AF7", "AF8", "TP10", "Right AUX"],
    },
];

/// Provider for BrainFlow-based board configurations.
pub struct BrainFlowProvider {
    config: BrainFlowConfig,
}

impl BrainFlowProvider {
    /// Create a new provider from configuration.
    pub fn new(config: BrainFlowConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl DeviceProvider for BrainFlowProvider {
    fn device_type(&self) -> DeviceType {
        DeviceType::Unknown("BrainFlow".to_string())
    }

    async fn is_available(&self) -> bool {
        true
    }

    /// Discover available boards.
    ///
    /// In **synthetic mode** (board_id 0, no serial port), returns one entry
    /// per entry in [`KNOWN_SYNTHETIC_BOARDS`] so callers can choose which
    /// hardware profile to simulate.  In all other configurations a single
    /// entry matching the configured board is returned.
    async fn discover(&self) -> Result<Vec<DeviceInfo>> {
        if is_synthetic_mode(&self.config) {
            return Ok(KNOWN_SYNTHETIC_BOARDS
                .iter()
                .map(|b| known_board_to_metadata(b).info)
                .collect());
        }
        let metadata = normalize_metadata(&self.config);
        Ok(vec![metadata.info])
    }

    async fn connect(
        &self,
        device_id: &DeviceId,
        _settings: Option<ConnectionSettings>,
    ) -> Result<Box<dyn Device>> {
        #[cfg(feature = "brainflow-native")]
        if self.config.board_id != 0 || self.config.serial_port.is_some() {
            if let Ok(native) = crate::brainflow_native::connect_native(
                self.config.board_id,
                self.config.serial_port.as_deref(),
                device_id,
            ) {
                return Ok(Box::new(native));
            }
            // Fall through to synthetic if native connect fails (e.g. no hardware)
        }

        if is_synthetic_mode(&self.config) {
            let board = KNOWN_SYNTHETIC_BOARDS
                .iter()
                .find(|b| synthetic_device_id(b.board_id) == device_id.0)
                .ok_or(DeviceError::NoDeviceFound)?;
            return Ok(Box::new(BrainFlowDevice::new(known_board_to_metadata(board))));
        }

        let metadata = normalize_metadata(&self.config);
        if metadata.info.id != *device_id {
            return Err(DeviceError::NoDeviceFound.into());
        }

        Ok(Box::new(BrainFlowDevice::new(metadata)))
    }
}

/// Returns `true` when the provider should operate in pure synthetic mode:
/// no real hardware target (board_id 0, no serial port).
fn is_synthetic_mode(config: &BrainFlowConfig) -> bool {
    config.board_id == 0 && config.serial_port.is_none()
}

/// Stable device ID for a synthetic board entry.
fn synthetic_device_id(board_id: i32) -> String {
    format!("brainflow::{}::synthetic", board_id)
}

/// Source ID for a synthetic board entry (colon-separated, no double-colon).
fn synthetic_source_id(board_id: i32) -> String {
    format!("brainflow:{}:synthetic", board_id)
}

/// Build a [`NormalizedMetadata`] from a statically-known board descriptor.
///
/// Uses the board's own channel names and specs rather than the generic
/// fallback values used for unknown/real boards.
fn known_board_to_metadata(board: &KnownBoard) -> NormalizedMetadata {
    let channels: Vec<ChannelConfig> = board
        .channel_names
        .iter()
        .map(|&name| ChannelConfig {
            id: ChannelId::new(name),
            position_10_20: Some(name.to_string()),
            enabled: true,
            reference: None,
        })
        .collect();

    let channel_config = DeviceChannelConfig {
        channels,
        sampling_rate_hz: board.sampling_rate_hz,
        resolution_bits: board.resolution_bits,
    };

    let info = DeviceInfo {
        id: DeviceId::new(synthetic_device_id(board.board_id)),
        device_type: normalized_device_type(board.board_id),
        name: Some(board.display_name.to_string()),
        firmware_version: None,
        channel_config: Some(channel_config.clone()),
        battery_percent: None,
        source_id: Some(synthetic_source_id(board.board_id)),
    };

    let mock_config = MockDeviceConfig {
        channel_count: board.channel_count,
        sampling_rate_hz: board.sampling_rate_hz,
        realistic_signal: true,
        seed: Some(board.board_id.max(0) as u64),
        signal_quality: 0.9,
        simulate_drops: false,
    };

    NormalizedMetadata {
        info,
        channel_config,
        mock_config,
    }
}

struct NormalizedMetadata {
    info: DeviceInfo,
    channel_config: DeviceChannelConfig,
    mock_config: MockDeviceConfig,
}

fn normalize_metadata(config: &BrainFlowConfig) -> NormalizedMetadata {
    let device_type = normalized_device_type(config.board_id);
    let display_name = normalized_board_name(config.board_id);
    let channel_count = normalized_channel_count(config.board_id);
    let sampling_rate_hz = normalized_sampling_rate_hz(config.board_id);

    let channel_config = DeviceChannelConfig {
        channels: normalized_channels(channel_count),
        sampling_rate_hz,
        resolution_bits: FALLBACK_RESOLUTION_BITS,
    };

    let board_token = normalize_token(&config.board_id.to_string());
    let port_token = normalize_token(config.serial_port.as_deref().unwrap_or("auto"));

    let id = DeviceId::new(format!("brainflow::{board_token}::{port_token}"));
    let source_id = Some(format!("brainflow:{board_token}:{port_token}"));
    let name = if let Some(port) = config.serial_port.as_deref() {
        Some(format!("{display_name} ({port})"))
    } else {
        Some(display_name)
    };

    let info = DeviceInfo {
        id: id.clone(),
        device_type,
        name,
        firmware_version: None,
        channel_config: Some(channel_config.clone()),
        battery_percent: None,
        source_id,
    };

    let mock_config = MockDeviceConfig {
        channel_count,
        sampling_rate_hz,
        realistic_signal: true,
        seed: Some(config.board_id.max(0) as u64),
        signal_quality: 0.9,
        simulate_drops: false,
    };

    NormalizedMetadata {
        info,
        channel_config,
        mock_config,
    }
}

fn normalized_device_type(board_id: i32) -> DeviceType {
    match board_id {
        1 => DeviceType::OpenBCICyton,
        2 => DeviceType::OpenBCIGanglion,
        0 => DeviceType::Unknown("BrainFlow/Synthetic".to_string()),
        other => DeviceType::Unknown(format!("BrainFlow/Board-{other}")),
    }
}

fn normalized_board_name(board_id: i32) -> String {
    match board_id {
        1 => "BrainFlow OpenBCI Cyton".to_string(),
        2 => "BrainFlow OpenBCI Ganglion".to_string(),
        0 => "BrainFlow Synthetic Board".to_string(),
        other => format!("BrainFlow Board {other}"),
    }
}

fn normalized_channel_count(board_id: i32) -> usize {
    match board_id {
        1 => 8,
        2 => 4,
        _ => FALLBACK_CHANNEL_COUNT,
    }
}

fn normalized_sampling_rate_hz(board_id: i32) -> f32 {
    match board_id {
        1 => 250.0,
        2 => 200.0,
        _ => FALLBACK_SAMPLING_RATE_HZ,
    }
}

fn normalized_channels(channel_count: usize) -> Vec<ChannelConfig> {
    (0..channel_count)
        .map(|idx| {
            let name = format!("EEG{}", idx + 1);
            ChannelConfig {
                id: ChannelId::new(&name),
                position_10_20: None,
                enabled: true,
                reference: None,
            }
        })
        .collect()
}

fn normalize_token(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "auto".to_string();
    }

    trimmed
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '_',
        })
        .collect()
}

/// Connected BrainFlow adapter device.
///
/// Internally this uses the mock signal generator while preserving normalized
/// BrainFlow metadata for identity and channel semantics.
pub struct BrainFlowDevice {
    info: DeviceInfo,
    channel_config: DeviceChannelConfig,
    inner: MockDevice,
}

impl BrainFlowDevice {
    fn new(metadata: NormalizedMetadata) -> Self {
        let inner = MockDevice::new(metadata.info.id.clone(), metadata.mock_config);
        Self {
            info: metadata.info,
            channel_config: metadata.channel_config,
            inner,
        }
    }
}

#[async_trait]
impl Device for BrainFlowDevice {
    fn id(&self) -> &DeviceId {
        &self.info.id
    }

    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn channel_config(&self) -> &DeviceChannelConfig {
        &self.channel_config
    }

    fn status(&self) -> DeviceStatus {
        self.inner.status()
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    fn is_streaming(&self) -> bool {
        self.inner.is_streaming()
    }

    async fn start_streaming(&mut self) -> Result<SampleStream> {
        self.inner.start_streaming().await
    }

    async fn stop_streaming(&mut self) -> Result<()> {
        self.inner.stop_streaming().await
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.inner.disconnect().await
    }

    fn status_stream(&self) -> Pin<Box<dyn Stream<Item = DeviceStatus> + Send>> {
        self.inner.status_stream()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyton_board_metadata_is_normalized() {
        let cfg = BrainFlowConfig {
            board_id: 1,
            serial_port: Some("COM7".to_string()),
        };

        let metadata = normalize_metadata(&cfg);
        let info = metadata.info;
        let channel_config = info.channel_config.expect("channel config should exist");

        assert_eq!(info.id.0, "brainflow::1::COM7");
        assert_eq!(info.source_id.as_deref(), Some("brainflow:1:COM7"));
        assert_eq!(info.device_type, DeviceType::OpenBCICyton);
        assert_eq!(channel_config.channels.len(), 8);
        assert!((channel_config.sampling_rate_hz - 250.0).abs() < f32::EPSILON);
    }

    #[test]
    fn unknown_board_id_falls_back_to_consistent_defaults() {
        let cfg = BrainFlowConfig {
            board_id: 999,
            serial_port: Some("/dev/ttyUSB0".to_string()),
        };

        let metadata = normalize_metadata(&cfg);
        let info = metadata.info;
        let channel_config = info.channel_config.expect("channel config should exist");

        assert_eq!(info.id.0, "brainflow::999::_dev_ttyUSB0");
        assert_eq!(
            info.source_id.as_deref(),
            Some("brainflow:999:_dev_ttyUSB0")
        );
        assert_eq!(
            info.device_type,
            DeviceType::Unknown("BrainFlow/Board-999".to_string())
        );
        assert_eq!(channel_config.channels.len(), FALLBACK_CHANNEL_COUNT);
        assert!((channel_config.sampling_rate_hz - FALLBACK_SAMPLING_RATE_HZ).abs() < f32::EPSILON);
    }

    #[tokio::test]
    async fn synthetic_mode_discovers_all_known_boards() {
        let provider = BrainFlowProvider::new(BrainFlowConfig::default());
        let devices = provider.discover().await.unwrap();

        assert_eq!(
            devices.len(),
            KNOWN_SYNTHETIC_BOARDS.len(),
            "should return one entry per known synthetic board"
        );

        // Every device ID should use the ::synthetic:: suffix
        for d in &devices {
            assert!(
                d.id.0.ends_with("::synthetic"),
                "expected synthetic ID suffix, got {}",
                d.id.0
            );
        }
    }

    #[tokio::test]
    async fn synthetic_mode_includes_insight_with_correct_channels() {
        let provider = BrainFlowProvider::new(BrainFlowConfig::default());
        let devices = provider.discover().await.unwrap();

        let insight = devices
            .iter()
            .find(|d| d.name.as_deref() == Some("Emotiv Insight (Synthetic)"))
            .expect("Emotiv Insight should be in synthetic board list");

        let cfg = insight.channel_config.as_ref().unwrap();
        assert_eq!(cfg.channels.len(), 5);
        assert!((cfg.sampling_rate_hz - 128.0).abs() < f32::EPSILON);
        let names: Vec<&str> = cfg
            .channels
            .iter()
            .map(|c| c.position_10_20.as_deref().unwrap_or(""))
            .collect();
        assert_eq!(names, ["AF3", "AF4", "T7", "T8", "Pz"]);
    }

    #[tokio::test]
    async fn synthetic_mode_connect_to_known_board_succeeds() {
        let provider = BrainFlowProvider::new(BrainFlowConfig::default());
        let devices = provider.discover().await.unwrap();

        // Connect to each discovered synthetic board
        for device_info in &devices {
            let device = provider.connect(&device_info.id, None).await;
            assert!(
                device.is_ok(),
                "connect to synthetic board '{}' should succeed",
                device_info.id
            );
        }
    }

    #[tokio::test]
    async fn synthetic_mode_connect_to_unknown_id_fails() {
        let provider = BrainFlowProvider::new(BrainFlowConfig::default());
        let result = provider
            .connect(&DeviceId::new("brainflow::99::synthetic"), None)
            .await;
        assert!(result.is_err(), "connecting to unknown synthetic ID should fail");
    }

    #[test]
    fn non_synthetic_config_not_in_synthetic_mode() {
        // board_id=0 WITH a serial port is not synthetic mode
        let cfg = BrainFlowConfig {
            board_id: 0,
            serial_port: Some("COM3".to_string()),
        };
        assert!(!is_synthetic_mode(&cfg));

        // board_id=1, no port is not synthetic mode
        let cfg = BrainFlowConfig {
            board_id: 1,
            serial_port: None,
        };
        assert!(!is_synthetic_mode(&cfg));
    }
}
