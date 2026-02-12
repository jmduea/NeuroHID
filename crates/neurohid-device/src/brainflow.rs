//! BrainFlow backend adapter with normalized board metadata.
//!
//! The native BrainFlow SDK integration is gated by build features in downstream
//! crates. This adapter provides a stable provider/device contract with
//! deterministic metadata so UI and routing behavior are consistent.

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

    async fn discover(&self) -> Result<Vec<DeviceInfo>> {
        let metadata = normalize_metadata(&self.config);
        Ok(vec![metadata.info])
    }

    async fn connect(
        &self,
        device_id: &DeviceId,
        _settings: Option<ConnectionSettings>,
    ) -> Result<Box<dyn Device>> {
        let metadata = normalize_metadata(&self.config);
        if metadata.info.id != *device_id {
            return Err(DeviceError::NoDeviceFound.into());
        }

        Ok(Box::new(BrainFlowDevice::new(metadata)))
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
}
