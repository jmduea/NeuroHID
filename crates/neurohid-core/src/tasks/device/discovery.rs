//! Discovery of device streams and provider creation.
//!
//! Handles scanning for available streams, mapping device info to discovered
//! stream state, and creating the appropriate device provider from config.

use neurohid_device::Device;
use neurohid_types::config::{DeviceBackend, DeviceConfig};
use neurohid_types::device::{DeviceId, DeviceInfo, DiscoveredStream};
use neurohid_types::error::{DeviceError, ExtensionError, Result};
use tokio::sync::RwLock;

#[cfg(feature = "brainflow")]
use neurohid_device::BrainFlowProvider;
#[cfg(feature = "device-lsl")]
use neurohid_device::LslProvider;
use neurohid_device::mock::MockProvider;
use neurohid_device::{DeviceProvider, MockDeviceConfig, SerialProvider};

use crate::extension_registry::ExtensionRegistry;
use crate::service::ServiceState;

use std::sync::Arc;

/// Scan for available streams and update `ServiceState::discovered_streams`.
pub(super) async fn scan(
    provider: &dyn DeviceProvider,
    state: &Arc<RwLock<ServiceState>>,
    connected_ids: &std::collections::HashSet<String>,
) {
    match provider.discover().await {
        Ok(devices) => {
            let mut discovered: Vec<DiscoveredStream> = devices
                .iter()
                .map(|info| device_info_to_discovered(info, connected_ids))
                .collect();

            if discovered.is_empty() {
                tracing::info!("Scan found 0 streams (is a publisher running?)");
            } else {
                tracing::info!("Scan found {} stream(s)", discovered.len());
            }

            let st = state.read().await;
            for ds in &mut discovered {
                if let Some(prev) = st.discovered_streams.iter().find(|p| p.id == ds.id) {
                    if ds.battery_percent.is_none() {
                        ds.battery_percent = prev.battery_percent;
                    }
                    if ds.channel_quality.is_none() {
                        ds.channel_quality = prev.channel_quality.clone();
                    }
                    if ds.effective_sample_rate_hz.is_none() {
                        ds.effective_sample_rate_hz = prev.effective_sample_rate_hz;
                    }
                    if ds.samples_received.is_none() {
                        ds.samples_received = prev.samples_received;
                    }
                    if ds.samples_dropped.is_none() {
                        ds.samples_dropped = prev.samples_dropped;
                    }
                    if ds.drop_rate_pct.is_none() {
                        ds.drop_rate_pct = prev.drop_rate_pct;
                    }
                    if ds.last_sample_age_ms.is_none() {
                        ds.last_sample_age_ms = prev.last_sample_age_ms;
                    }
                    if ds.preprocessing_summary.is_none() {
                        ds.preprocessing_summary = prev.preprocessing_summary.clone();
                    }
                    if ds.integrity_state.is_none() {
                        ds.integrity_state = prev.integrity_state.clone();
                    }
                }
            }
            drop(st);

            let mut st = state.write().await;
            st.discovered_streams = discovered;
        }
        Err(e) => {
            tracing::warn!("Stream scan failed: {}", e);
        }
    }
}

/// Convert a `DeviceInfo` into a `DiscoveredStream`.
fn device_info_to_discovered(
    info: &DeviceInfo,
    connected_ids: &std::collections::HashSet<String>,
) -> DiscoveredStream {
    let id = info.id.0.clone();
    let name = info.name.clone().unwrap_or_else(|| info.id.0.clone());

    let (stream_type, channel_count, sample_rate) = match &info.channel_config {
        Some(cfg) => {
            let type_str = match &info.device_type {
                neurohid_types::device::DeviceType::Unknown(s) => s.clone(),
                other => format!("{:?}", other),
            };
            (
                type_str,
                cfg.channels.len() as i32,
                cfg.sampling_rate_hz as f64,
            )
        }
        None => {
            let type_str = match &info.device_type {
                neurohid_types::device::DeviceType::Unknown(s) => s.clone(),
                other => format!("{:?}", other),
            };
            (type_str, 0, 0.0)
        }
    };

    let connected = connected_ids.contains(&id);

    DiscoveredStream {
        id,
        name,
        stream_type,
        channel_count,
        sample_rate,
        connected,
        battery_percent: info.battery_percent,
        channel_quality: None,
        source_id: info.source_id.clone(),
        effective_sample_rate_hz: None,
        samples_received: None,
        samples_dropped: None,
        drop_rate_pct: None,
        last_sample_age_ms: None,
        preprocessing_summary: None,
        integrity_state: None,
    }
}

/// Create a device provider based on the backend configuration.
pub(super) async fn create_provider(
    config: &DeviceConfig,
    registry: Option<&ExtensionRegistry>,
) -> Result<Box<dyn DeviceProvider>> {
    match &config.backend {
        DeviceBackend::Mock => {
            tracing::info!("Using Mock device backend");
            Ok(Box::new(MockProvider::new(MockDeviceConfig::default())))
        }

        DeviceBackend::Lsl => {
            tracing::info!("Using LSL device backend");
            create_lsl_provider(config)
        }

        DeviceBackend::Auto => {
            tracing::info!(
                "Using Auto device backend (LSL preferred, BrainFlow synthetic fallback)"
            );
            let fallback = create_brainflow_provider(config)?;

            match create_lsl_provider(config) {
                Ok(lsl) => Ok(Box::new(AutoProvider::new(lsl, fallback))),
                Err(e) => {
                    tracing::warn!(
                        "Auto backend: LSL unavailable ({e}), using BrainFlow synthetic only"
                    );
                    Ok(fallback)
                }
            }
        }

        DeviceBackend::Serial => {
            tracing::info!("Using Serial device backend");
            create_serial_provider(config)
        }

        DeviceBackend::BrainFlow => {
            tracing::info!("Using BrainFlow device backend");
            create_brainflow_provider(config)
        }

        DeviceBackend::Extension(name) => {
            let reg = registry.ok_or_else(|| ExtensionError::LoadError {
                name: name.clone(),
                reason: "extension registry not available (device extension requires registry)"
                    .to_string(),
            })?;
            tracing::info!(name = %name, "Using device extension");
            let loaded = reg.load_device_provider(name)?;
            Ok(Box::new(loaded))
        }
    }
}

#[cfg(feature = "device-lsl")]
fn create_lsl_provider(config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    let lsl_config = config.lsl.clone().unwrap_or_default();
    Ok(Box::new(LslProvider::new(lsl_config)))
}

#[cfg(not(feature = "device-lsl"))]
fn create_lsl_provider(_config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    Err(DeviceError::UnsupportedDevice {
        device_type: "LSL backend requires the `device-lsl` feature".to_string(),
    }
    .into())
}

fn create_serial_provider(config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    let serial_config = config.serial.clone().unwrap_or_default();
    Ok(Box::new(SerialProvider::new(serial_config)))
}

#[cfg(feature = "brainflow")]
fn create_brainflow_provider(config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    let brainflow_config = config.brainflow.clone().unwrap_or_default();
    Ok(Box::new(BrainFlowProvider::new(brainflow_config)))
}

#[cfg(not(feature = "brainflow"))]
fn create_brainflow_provider(_config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    Err(DeviceError::UnsupportedDevice {
        device_type: "BrainFlow backend requires the `brainflow` feature".to_string(),
    }
    .into())
}

/// A composite provider that delegates to LSL first, falling back to BrainFlow synthetic
/// on each `discover()` / `connect()` call instead of choosing once at startup.
struct AutoProvider {
    lsl: Box<dyn DeviceProvider>,
    fallback: Box<dyn DeviceProvider>,
}

impl AutoProvider {
    fn new(lsl: Box<dyn DeviceProvider>, fallback: Box<dyn DeviceProvider>) -> Self {
        Self { lsl, fallback }
    }
}

#[async_trait::async_trait]
impl DeviceProvider for AutoProvider {
    fn device_type(&self) -> neurohid_types::device::DeviceType {
        neurohid_types::device::DeviceType::Unknown("Auto".into())
    }

    async fn is_available(&self) -> bool {
        true
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>> {
        match self.lsl.discover().await {
            Ok(devices) if !devices.is_empty() => {
                tracing::debug!(
                    "Auto: LSL discovered {} stream(s), using LSL",
                    devices.len()
                );
                Ok(devices)
            }
            Ok(_) => {
                tracing::debug!("Auto: no LSL streams found, falling back to BrainFlow synthetic");
                self.fallback.discover().await
            }
            Err(e) => {
                tracing::warn!(
                    "Auto: LSL discover failed ({e}), falling back to BrainFlow synthetic"
                );
                self.fallback.discover().await
            }
        }
    }

    async fn connect(
        &self,
        device_id: &DeviceId,
        settings: Option<neurohid_types::device::ConnectionSettings>,
    ) -> Result<Box<dyn Device>> {
        match self.lsl.connect(device_id, settings.clone()).await {
            Ok(device) => Ok(device),
            Err(e) => {
                tracing::warn!(
                    "Auto: LSL connect to '{}' failed ({e}), trying fallback (BrainFlow synthetic)",
                    device_id
                );
                self.fallback.connect(device_id, settings).await
            }
        }
    }
}
