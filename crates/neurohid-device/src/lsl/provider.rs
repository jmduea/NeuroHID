//! LSL device provider — discovers and connects to LSL streams.

use async_trait::async_trait;

use neurohid_types::{
    config::LslConfig,
    device::{ConnectionSettings, DeviceId, DeviceInfo, DeviceType},
    error::{DeviceError, Result},
    signal::{ChannelConfig, ChannelId, DeviceChannelConfig},
};

use crate::traits::{Device, DeviceProvider};

/// A device provider that discovers LSL streams on the local network.
pub struct LslProvider {
    config: LslConfig,
}

impl LslProvider {
    pub fn new(config: LslConfig) -> Self {
        super::configure_lsl();
        tracing::info!(
            "LSL provider initialized (liblsl v{}.{}, predicate: {})",
            lsl::library_version() / 100,
            lsl::library_version() % 100,
            if config.predicate.is_empty() {
                "<all streams>"
            } else {
                &config.predicate
            },
        );
        Self { config }
    }
}

/// Resolve LSL streams, correctly handling an empty predicate.
///
/// liblsl's `lsl_resolve_bypred` passes the predicate through `build_query`,
/// which appends `" and " + pred` when the pointer is non-null — even if the
/// string is empty.  This produces a malformed XPath query like
/// `session_id='default' and ` that silently matches nothing.
///
/// When the predicate is empty (the default) we call `resolve_streams` instead,
/// which is the same code path LabRecorder uses and avoids the bug entirely.
fn resolve_lsl(
    predicate: &str,
    minimum: i32,
    timeout: f64,
) -> std::result::Result<Vec<lsl::StreamInfo>, lsl::Error> {
    if predicate.is_empty() {
        lsl::resolve_streams(timeout)
    } else {
        lsl::resolve_bypred(predicate, minimum, timeout)
    }
}

/// Data extracted from an LSL `StreamInfo` (Send-safe, no C pointers).
#[derive(Clone)]
struct ResolvedStream {
    name: String,
    stream_type: String,
    channel_count: i32,
    nominal_srate: f64,
    source_id: String,
}

impl ResolvedStream {
    fn device_id(&self) -> DeviceId {
        DeviceId::new(Self::make_id(&self.source_id, &self.name))
    }

    /// Build a composite ID from source_id and stream name.
    ///
    /// Emotiv (and other multi-stream publishers) share a single `source_id`
    /// across all of their LSL streams. Using `source_id::stream_name` as
    /// the key gives every stream a unique identity.
    fn make_id(source_id: &str, name: &str) -> String {
        if source_id.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", source_id, name)
        }
    }

    fn to_device_info(&self) -> DeviceInfo {
        let channel_count = self.channel_count.max(0) as usize;

        let channels: Vec<ChannelConfig> = (0..channel_count)
            .map(|i| {
                let name = format!("Ch{}", i);
                ChannelConfig {
                    id: ChannelId::new(&name),
                    position_10_20: None,
                    enabled: true,
                    reference: None,
                }
            })
            .collect();

        let channel_config = DeviceChannelConfig {
            channels,
            sampling_rate_hz: self.nominal_srate as f32,
            resolution_bits: 32, // Float32
        };

        DeviceInfo {
            id: self.device_id(),
            device_type: DeviceType::Unknown(format!("{}/{}", self.stream_type, self.name)),
            name: Some(self.name.clone()),
            firmware_version: None,
            channel_config: Some(channel_config),
            battery_percent: None,
        }
    }
}

#[async_trait]
impl DeviceProvider for LslProvider {
    fn device_type(&self) -> DeviceType {
        DeviceType::Unknown("LSL".into())
    }

    async fn is_available(&self) -> bool {
        let predicate = self.config.predicate.clone();
        tokio::task::spawn_blocking(move || {
            resolve_lsl(&predicate, 1, 1.0)
                .map(|streams| !streams.is_empty())
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>> {
        let predicate = self.config.predicate.clone();
        let timeout = self.config.resolve_timeout_secs;

        let resolved = tokio::task::spawn_blocking(move || {
            let streams = resolve_lsl(&predicate, 1, timeout)
                .map_err(|e| DeviceError::CommunicationError(format!("LSL resolve failed: {e}")))?;

            Ok::<_, DeviceError>(
                streams
                    .iter()
                    .map(|s| ResolvedStream {
                        name: s.stream_name(),
                        stream_type: s.stream_type(),
                        channel_count: s.channel_count(),
                        nominal_srate: s.nominal_srate(),
                        source_id: s.source_id(),
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .await
        .map_err(|e| {
            DeviceError::CommunicationError(format!("LSL resolve task panicked: {e}"))
        })??;

        if resolved.is_empty() {
            return Ok(Vec::new());
        }

        let devices: Vec<DeviceInfo> = resolved.iter().map(|r| r.to_device_info()).collect();

        tracing::info!("LSL: discovered {} stream(s)", devices.len());
        for d in &devices {
            tracing::debug!("  - {} ({})", d.name.as_deref().unwrap_or("?"), d.id);
        }

        Ok(devices)
    }

    async fn connect(
        &self,
        device_id: &DeviceId,
        _settings: Option<ConnectionSettings>,
    ) -> Result<Box<dyn Device>> {
        let timeout = self.config.resolve_timeout_secs;
        let target_id = device_id.0.clone();
        let buffer_size = self.config.buffer_size as i32;

        // Extract the stream name from the composite ID ("source_id::name")
        // so we can use a targeted LSL predicate that returns immediately
        // once the single matching stream is found, instead of waiting the
        // full timeout to enumerate every stream on the network.
        let stream_name = if let Some((_src, name)) = target_id.split_once("::") {
            name.to_string()
        } else {
            target_id.clone()
        };

        let (inlet, resolved) = tokio::task::spawn_blocking(move || {
            // Targeted resolve: ask liblsl for one stream with this exact name.
            // resolve_bypred returns as soon as `minimum` (1) matches are found.
            // Use a short timeout since discover() already confirmed the stream
            // exists — we just need the StreamInfo handle for inlet creation.
            let pred = format!("name='{}'", stream_name);
            let connect_timeout = timeout.min(0.5);
            let streams = lsl::resolve_bypred(&pred, 1, connect_timeout)
                .map_err(|e| DeviceError::CommunicationError(format!("LSL resolve failed: {e}")))?;

            let stream_info = streams
                .into_iter()
                .find(|s| {
                    let id = ResolvedStream::make_id(&s.source_id(), &s.stream_name());
                    id == target_id
                })
                .ok_or(DeviceError::NoDeviceFound)?;

            let resolved = ResolvedStream {
                name: stream_info.stream_name(),
                stream_type: stream_info.stream_type(),
                channel_count: stream_info.channel_count(),
                nominal_srate: stream_info.nominal_srate(),
                source_id: stream_info.source_id(),
            };

            let max_buflen = if buffer_size > 0 { buffer_size } else { 360 };
            let inlet = lsl::StreamInlet::new(&stream_info, max_buflen, 0, true).map_err(|e| {
                DeviceError::ConnectionFailed {
                    reason: format!("LSL inlet creation failed: {e}"),
                }
            })?;

            Ok::<_, DeviceError>((super::device::SendInlet(inlet), resolved))
        })
        .await
        .map_err(|e| {
            DeviceError::CommunicationError(format!("LSL connect task panicked: {e}"))
        })??;

        let device_info = resolved.to_device_info();
        tracing::info!(
            "LSL: connected to stream '{}' ({} ch @ {} Hz)",
            device_info.name.as_deref().unwrap_or("?"),
            resolved.channel_count,
            resolved.nominal_srate
        );

        let device = super::device::LslDevice::new(inlet, device_info);
        Ok(Box::new(device))
    }
}
