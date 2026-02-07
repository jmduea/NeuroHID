//! # Emotiv Device Provider
//!
//! Implements [`DeviceProvider`] for Emotiv headsets using the Cortex API.
//!
//! ## Supported Headsets
//!
//! | Headset      | Channels | Sample Rate | Notes                    |
//! |-------------|----------|-------------|--------------------------|
//! | Insight     | 5        | 128 Hz      | AF3, AF4, T7, T8, Pz     |
//! | EPOC+       | 14       | 128 Hz      | Full 10-20 coverage      |
//! | EPOC X      | 14       | 256 Hz      | High-resolution variant  |
//!
//! ## Authentication
//!
//! The provider requires a `client_id` and `client_secret` obtained from
//! the [Emotiv Developer Portal](https://www.emotiv.com/developer/). These
//! are passed in at construction time (loaded from the platform keyring
//! by `neurohid-core`).

use async_trait::async_trait;

use neurohid_types::config::EmotivConfig;
use neurohid_types::device::{ConnectionSettings, DeviceId, DeviceInfo, DeviceType};
use neurohid_types::error::{DeviceError, Result};
use neurohid_types::signal::{ChannelConfig, ChannelId, DeviceChannelConfig};

use crate::traits::{Device, DeviceProvider};

use super::cortex_client::CortexClient;
use super::device::EmotivDevice;
use super::protocol::HeadsetInfo;

/// Device provider for Emotiv headsets via the Cortex API.
///
/// The Emotiv Cortex service must be running locally (it comes with the
/// Emotiv Launcher). The provider connects to it via WebSocket to
/// discover headsets, authenticate, and create sessions.
pub struct EmotivProvider {
    config: EmotivConfig,
    client_id: String,
    client_secret: String,
}

impl EmotivProvider {
    /// Create a new Emotiv provider.
    ///
    /// # Arguments
    ///
    /// * `config` - Emotiv-specific configuration (URL, license, etc.)
    /// * `client_id` - Cortex API client ID from the Emotiv Developer Portal
    /// * `client_secret` - Cortex API client secret
    pub fn new(config: EmotivConfig, client_id: String, client_secret: String) -> Self {
        Self {
            config,
            client_id,
            client_secret,
        }
    }
}

#[async_trait]
impl DeviceProvider for EmotivProvider {
    fn device_type(&self) -> DeviceType {
        DeviceType::EmotivInsight
    }

    async fn is_available(&self) -> bool {
        // Connect to the Cortex WebSocket and call getCortexInfo to verify
        // the API is actually responding (not just accepting TCP connections).
        match CortexClient::connect(&self.config.cortex_url).await {
            Ok(mut client) => {
                let available = client.get_cortex_info().await.is_ok();
                let _ = client.disconnect().await;
                if !available {
                    tracing::warn!(
                        cortex_url = %self.config.cortex_url,
                        "Cortex WebSocket connected but API not responding (getCortexInfo failed)"
                    );
                }
                available
            }
            Err(e) => {
                tracing::warn!(
                    cortex_url = %self.config.cortex_url,
                    error = %e,
                    "Emotiv Cortex service is not reachable"
                );
                false
            }
        }
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>> {
        let mut client = CortexClient::connect(&self.config.cortex_url).await?;

        // Authenticate (required for queryHeadsets)
        client
            .authenticate(&self.client_id, &self.client_secret)
            .await?;

        // Trigger headset scanning before querying (documented as required)
        if let Err(e) = client.refresh_headsets().await {
            tracing::warn!("controlDevice(refresh) failed: {}, querying anyway", e);
        }
        // Brief pause to let the scan discover nearby devices
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let headsets = client.query_headsets().await?;

        let devices = headsets
            .iter()
            .map(|h| {
                let device_type = infer_device_type(h);
                let channel_config = build_channel_config(&device_type);

                DeviceInfo {
                    id: DeviceId::new(&h.id),
                    device_type,
                    name: Some(h.id.clone()),
                    firmware_version: h.firmware.clone(),
                    channel_config: Some(channel_config),
                    battery_percent: None,
                }
            })
            .collect();

        // Drop the client — a new one is created for connect()
        let _ = client.disconnect().await;

        Ok(devices)
    }

    async fn connect(
        &self,
        device_id: &DeviceId,
        settings: Option<ConnectionSettings>,
    ) -> Result<Box<dyn Device>> {
        let _ = settings; // Reserved for future use

        let mut client = CortexClient::connect(&self.config.cortex_url).await?;

        // Authenticate
        let cortex_token = client
            .authenticate(&self.client_id, &self.client_secret)
            .await?;

        // Trigger headset scanning before querying
        if let Err(e) = client.refresh_headsets().await {
            tracing::warn!("controlDevice(refresh) failed: {}, querying anyway", e);
        }

        // Ensure the headset is connected (not just discovered)
        let headsets = client.query_headsets().await?;
        let headset = headsets
            .iter()
            .find(|h| h.id == device_id.0)
            .ok_or(DeviceError::NoDeviceFound)?;

        if headset.status != "connected" {
            tracing::info!(
                headset = %device_id.0,
                status = %headset.status,
                "Headset not connected, initiating connection"
            );
            client.connect_headset(&device_id.0).await?;

            // Poll until connected (up to 30 seconds)
            let connected = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                poll_headset_connected(&mut client, &device_id.0),
            )
            .await
            .map_err(|_| DeviceError::Timeout)??;

            if !connected {
                return Err(DeviceError::ConnectionFailed {
                    reason: "Headset did not reach connected state".into(),
                }
                .into());
            }
        }

        // Re-query to get updated headset info
        let headsets = client.query_headsets().await?;
        let headset = headsets
            .iter()
            .find(|h| h.id == device_id.0)
            .ok_or(DeviceError::NoDeviceFound)?;

        let device_type = infer_device_type(headset);
        let channel_config = build_channel_config(&device_type);

        // Create session
        let session = client.create_session(&cortex_token, &device_id.0).await?;

        let device = EmotivDevice::new(
            device_id.clone(),
            device_type,
            channel_config,
            client,
            session.id,
            cortex_token,
            headset.clone(),
        );

        Ok(Box::new(device))
    }
}

/// Poll until a headset reaches "connected" status.
async fn poll_headset_connected(client: &mut CortexClient, headset_id: &str) -> Result<bool> {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let headsets = client.query_headsets().await?;
        if let Some(h) = headsets.iter().find(|h| h.id == headset_id) {
            if h.status == "connected" {
                return Ok(true);
            }
            tracing::debug!(status = %h.status, "Waiting for headset connection...");
        } else {
            return Ok(false); // Headset disappeared
        }
    }
}

/// Infer the NeuroHID DeviceType from an Emotiv headset ID.
///
/// Emotiv headset IDs follow patterns like:
/// - `INSIGHT-XXXXXXXX` → EmotivInsight
/// - `EPOCPLUS-XXXXXXXX` → EmotivEpocPlus
/// - `EPOCFLEX-XXXXXXXX` → EmotivEpocPlus (same channel layout)
/// - `EPOCX-XXXXXXXX` → EmotivEpocX
pub fn infer_device_type(headset: &HeadsetInfo) -> DeviceType {
    let id_upper = headset.id.to_uppercase();
    if id_upper.starts_with("INSIGHT") {
        DeviceType::EmotivInsight
    } else if id_upper.starts_with("EPOCX") || id_upper.starts_with("EPOC-X") {
        DeviceType::EmotivEpocX
    } else if id_upper.starts_with("EPOCPLUS")
        || id_upper.starts_with("EPOC+")
        || id_upper.starts_with("EPOCFLEX")
    {
        DeviceType::EmotivEpocPlus
    } else if id_upper.starts_with("EPOC") {
        // Generic EPOC — assume EPOC+ layout
        DeviceType::EmotivEpocPlus
    } else {
        DeviceType::Unknown(format!("Emotiv {}", headset.id))
    }
}

/// Build the channel configuration for a given Emotiv device type.
pub fn build_channel_config(device_type: &DeviceType) -> DeviceChannelConfig {
    match device_type {
        DeviceType::EmotivInsight => {
            let names = ["AF3", "AF4", "T7", "T8", "Pz"];
            DeviceChannelConfig {
                channels: names
                    .iter()
                    .map(|n| ChannelConfig {
                        id: ChannelId::new(*n),
                        position_10_20: Some(n.to_string()),
                        enabled: true,
                        reference: None,
                    })
                    .collect(),
                sampling_rate_hz: 128.0,
                resolution_bits: 14,
            }
        }
        DeviceType::EmotivEpocPlus | DeviceType::EmotivEpocX => {
            let names = [
                "AF3", "F7", "F3", "FC5", "T7", "P7", "O1", "O2", "P8", "T8", "FC6", "F4", "F8",
                "AF4",
            ];
            let sampling_rate = if matches!(device_type, DeviceType::EmotivEpocX) {
                256.0
            } else {
                128.0
            };
            DeviceChannelConfig {
                channels: names
                    .iter()
                    .map(|n| ChannelConfig {
                        id: ChannelId::new(*n),
                        position_10_20: Some(n.to_string()),
                        enabled: true,
                        reference: None,
                    })
                    .collect(),
                sampling_rate_hz: sampling_rate,
                resolution_bits: 14,
            }
        }
        _ => {
            // Fallback: Insight-like layout
            build_channel_config(&DeviceType::EmotivInsight)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_device_type_insight() {
        let info = HeadsetInfo {
            id: "INSIGHT-A1B2C3D4".into(),
            dongle_serial: None,
            firmware: None,
            status: "connected".into(),
            connected_by: None,
            motion_sensors: None,
            sensors: None,
            settings: None,
        };
        assert_eq!(infer_device_type(&info), DeviceType::EmotivInsight);
    }

    #[test]
    fn test_infer_device_type_epocx() {
        let info = HeadsetInfo {
            id: "EPOCX-12345678".into(),
            dongle_serial: None,
            firmware: None,
            status: "connected".into(),
            connected_by: None,
            motion_sensors: None,
            sensors: None,
            settings: None,
        };
        assert_eq!(infer_device_type(&info), DeviceType::EmotivEpocX);
    }

    #[test]
    fn test_infer_device_type_epocplus() {
        let info = HeadsetInfo {
            id: "EPOCPLUS-AABBCCDD".into(),
            dongle_serial: None,
            firmware: None,
            status: "connected".into(),
            connected_by: None,
            motion_sensors: None,
            sensors: None,
            settings: None,
        };
        assert_eq!(infer_device_type(&info), DeviceType::EmotivEpocPlus);
    }

    #[test]
    fn test_channel_config_insight() {
        let config = build_channel_config(&DeviceType::EmotivInsight);
        assert_eq!(config.channels.len(), 5);
        assert_eq!(config.sampling_rate_hz, 128.0);
    }

    #[test]
    fn test_channel_config_epocx() {
        let config = build_channel_config(&DeviceType::EmotivEpocX);
        assert_eq!(config.channels.len(), 14);
        assert_eq!(config.sampling_rate_hz, 256.0);
    }

    #[test]
    fn test_channel_config_epocplus() {
        let config = build_channel_config(&DeviceType::EmotivEpocPlus);
        assert_eq!(config.channels.len(), 14);
        assert_eq!(config.sampling_rate_hz, 128.0);
    }
}
