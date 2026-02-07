//! # Mock Device Implementation
//!
//! This module provides a simulated device for testing and development.
//! It generates synthetic EEG-like signals without requiring actual hardware,
//! making it invaluable for:
//!
//! - Development when you don't have a device handy
//! - Automated testing in CI/CD pipelines
//! - Demonstrating functionality to users before they have hardware
//! - Debugging signal processing without hardware variables
//!
//! The mock device can generate either pure noise or more realistic signals
//! that mimic real EEG characteristics (alpha rhythms, artifacts, etc.).

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::task::{Context, Poll};
use tokio::sync::watch;
use tokio::time::{interval, Duration, Interval};

use neurohid_types::{
    device::{DeviceId, DeviceInfo, DeviceStatus, DeviceType, ConnectionState, ConnectionSettings},
    signal::{Sample, DeviceChannelConfig, ChannelConfig, ChannelId},
    error::{DeviceError, Result},
    now_micros,
};

use crate::traits::{Device, DeviceProvider, SampleStream};

/// Configuration for creating a mock device (useful for testing and development).
#[derive(Debug, Clone)]
pub struct MockDeviceConfig {
    /// Number of channels to simulate.
    pub channel_count: usize,

    /// Sampling rate in Hz.
    pub sampling_rate_hz: f32,

    /// Whether to simulate realistic signal characteristics.
    pub realistic_signal: bool,

    /// Optional seed for reproducible random signals.
    pub seed: Option<u64>,

    /// Simulated signal quality (0.0 to 1.0).
    pub signal_quality: f32,

    /// Whether to simulate occasional connection drops.
    pub simulate_drops: bool,
}

impl Default for MockDeviceConfig {
    fn default() -> Self {
        Self {
            channel_count: 5, // Matches Emotiv Insight
            sampling_rate_hz: 128.0,
            realistic_signal: true,
            seed: None,
            signal_quality: 0.9,
            simulate_drops: false,
        }
    }
}

/// A mock device provider that creates simulated devices.
///
/// This provider always "discovers" a configurable number of mock devices
/// and can connect to any of them instantly.
pub struct MockProvider {
    config: MockDeviceConfig,
    num_devices: usize,
}

impl MockProvider {
    /// Creates a new mock provider with the given configuration.
    ///
    /// The configuration controls the characteristics of devices created
    /// by this provider (channel count, sampling rate, signal realism, etc.).
    pub fn new(config: MockDeviceConfig) -> Self {
        Self {
            config,
            num_devices: 1,
        }
    }
    
    /// Sets the number of devices this provider will "discover".
    pub fn with_num_devices(mut self, count: usize) -> Self {
        self.num_devices = count;
        self
    }
}

#[async_trait]
impl DeviceProvider for MockProvider {
    fn device_type(&self) -> DeviceType {
        DeviceType::Mock
    }
    
    async fn is_available(&self) -> bool {
        // Mock devices are always available - that's the point!
        true
    }
    
    async fn discover(&self) -> Result<Vec<DeviceInfo>> {
        // Generate the configured number of mock devices
        let devices: Vec<DeviceInfo> = (0..self.num_devices)
            .map(|i| {
                let id = DeviceId::new(format!("mock_device_{}", i));
                DeviceInfo {
                    id,
                    device_type: DeviceType::Mock,
                    name: Some(format!("Mock Device {}", i)),
                    firmware_version: Some("1.0.0-mock".to_string()),
                    channel_config: Some(self.create_channel_config()),
                    battery_percent: Some(100),
                }
            })
            .collect();
        
        Ok(devices)
    }
    
    async fn connect(
        &self,
        device_id: &DeviceId,
        _settings: Option<ConnectionSettings>,
    ) -> Result<Box<dyn Device>> {
        // Verify the device ID looks like one we'd create
        if !device_id.0.starts_with("mock_device_") {
            return Err(DeviceError::NoDeviceFound.into());
        }
        
        // Create the mock device
        let device = MockDevice::new(device_id.clone(), self.config.clone());
        
        Ok(Box::new(device))
    }
}

impl MockProvider {
    fn create_channel_config(&self) -> DeviceChannelConfig {
        // Create channel configs that mimic an Emotiv Insight
        let channel_names = ["AF3", "AF4", "T7", "T8", "Pz"];
        
        let channels: Vec<ChannelConfig> = (0..self.config.channel_count)
            .map(|i| {
                let name = if i < channel_names.len() {
                    channel_names[i].to_string()
                } else {
                    format!("Ch{}", i)
                };
                
                ChannelConfig {
                    id: ChannelId::new(&name),
                    position_10_20: Some(name),
                    enabled: true,
                    reference: None,
                }
            })
            .collect();
        
        DeviceChannelConfig {
            channels,
            sampling_rate_hz: self.config.sampling_rate_hz,
            resolution_bits: 14,
        }
    }
}

/// A simulated biosensor device.
///
/// This device generates synthetic signals based on its configuration.
/// It supports all the standard device operations (connect, stream, disconnect)
/// and can optionally simulate connection drops and variable signal quality.
pub struct MockDevice {
    id: DeviceId,
    info: DeviceInfo,
    config: MockDeviceConfig,
    channel_config: DeviceChannelConfig,
    
    // State tracking
    connected: AtomicBool,
    streaming: Arc<AtomicBool>,
    samples_sent: AtomicU64,
    
    // Status broadcasting
    status_tx: watch::Sender<DeviceStatus>,
    status_rx: watch::Receiver<DeviceStatus>,
}

impl MockDevice {
    /// Creates a new mock device with the given configuration.
    pub fn new(id: DeviceId, config: MockDeviceConfig) -> Self {
        let channel_config = Self::create_channel_config(&config);
        
        let info = DeviceInfo {
            id: id.clone(),
            device_type: DeviceType::Mock,
            name: Some("Mock Device".to_string()),
            firmware_version: Some("1.0.0-mock".to_string()),
            channel_config: Some(channel_config.clone()),
            battery_percent: Some(100),
        };
        
        let initial_status = DeviceStatus {
            device_id: id.clone(),
            connection_state: ConnectionState::Connected,
            is_streaming: false,
            samples_received: 0,
            samples_dropped: 0,
            battery_percent: Some(100),
            channel_quality: Some(vec![config.signal_quality; config.channel_count]),
            message: Some("Mock device ready".to_string()),
        };
        
        let (status_tx, status_rx) = watch::channel(initial_status);
        
        Self {
            id,
            info,
            config,
            channel_config,
            connected: AtomicBool::new(true),
            streaming: Arc::new(AtomicBool::new(false)),
            samples_sent: AtomicU64::new(0),
            status_tx,
            status_rx,
        }
    }
    
    fn create_channel_config(config: &MockDeviceConfig) -> DeviceChannelConfig {
        let channel_names = ["AF3", "AF4", "T7", "T8", "Pz"];
        
        let channels: Vec<ChannelConfig> = (0..config.channel_count)
            .map(|i| {
                let name = if i < channel_names.len() {
                    channel_names[i].to_string()
                } else {
                    format!("Ch{}", i)
                };
                
                ChannelConfig {
                    id: ChannelId::new(&name),
                    position_10_20: Some(name),
                    enabled: true,
                    reference: None,
                }
            })
            .collect();
        
        DeviceChannelConfig {
            channels,
            sampling_rate_hz: config.sampling_rate_hz,
            resolution_bits: 14,
        }
    }
    
    fn update_status(&self) {
        let status = DeviceStatus {
            device_id: self.id.clone(),
            connection_state: if self.connected.load(Ordering::SeqCst) {
                ConnectionState::Connected
            } else {
                ConnectionState::Disconnected
            },
            is_streaming: self.streaming.load(Ordering::SeqCst),
            samples_received: self.samples_sent.load(Ordering::SeqCst),
            samples_dropped: 0,
            battery_percent: Some(100),
            channel_quality: Some(vec![self.config.signal_quality; self.config.channel_count]),
            message: None,
        };
        
        // Ignore send errors (receiver might be dropped)
        let _ = self.status_tx.send(status);
    }
}

#[async_trait]
impl Device for MockDevice {
    fn id(&self) -> &DeviceId {
        &self.id
    }
    
    fn info(&self) -> &DeviceInfo {
        &self.info
    }
    
    fn channel_config(&self) -> &DeviceChannelConfig {
        &self.channel_config
    }
    
    fn status(&self) -> DeviceStatus {
        self.status_rx.borrow().clone()
    }
    
    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }
    
    fn is_streaming(&self) -> bool {
        self.streaming.load(Ordering::SeqCst)
    }
    
    async fn start_streaming(&mut self) -> Result<SampleStream> {
        if !self.connected.load(Ordering::SeqCst) {
            return Err(DeviceError::NotConnected.into());
        }
        
        if self.streaming.load(Ordering::SeqCst) {
            return Err(DeviceError::DeviceBusy.into());
        }
        
        self.streaming.store(true, Ordering::SeqCst);
        self.update_status();
        
        // Calculate sample interval from sampling rate
        let sample_period_us = (1_000_000.0 / self.config.sampling_rate_hz) as u64;
        
        // Create the sample stream
        let stream = MockSampleStream::new(
            self.config.clone(),
            Arc::clone(&self.streaming),
            sample_period_us,
        );
        
        Ok(Box::pin(stream))
    }
    
    async fn stop_streaming(&mut self) -> Result<()> {
        self.streaming.store(false, Ordering::SeqCst);
        self.update_status();
        Ok(())
    }
    
    async fn disconnect(&mut self) -> Result<()> {
        self.streaming.store(false, Ordering::SeqCst);
        self.connected.store(false, Ordering::SeqCst);
        self.update_status();
        Ok(())
    }
    
    fn status_stream(&self) -> Pin<Box<dyn Stream<Item = DeviceStatus> + Send>> {
        let rx = self.status_rx.clone();
        Box::pin(futures::stream::unfold(rx, |mut rx| async move {
            rx.changed().await.ok()?;
            let status = rx.borrow().clone();
            Some((status, rx))
        }))
    }
}

/// A stream that yields mock samples at the configured rate.
struct MockSampleStream {
    config: MockDeviceConfig,
    streaming: Arc<AtomicBool>,
    interval: Interval,
    sequence: u64,
}

impl MockSampleStream {
    fn new(
        config: MockDeviceConfig,
        streaming: Arc<AtomicBool>,
        sample_period_us: u64,
    ) -> Self {
        Self {
            config,
            streaming,
            interval: interval(Duration::from_micros(sample_period_us)),
            sequence: 0,
        }
    }
    
    fn generate_sample(&mut self) -> Sample {
        let timestamp = now_micros();
        self.sequence += 1;
        
        let values: Vec<f32> = (0..self.config.channel_count)
            .map(|ch| {
                if !self.config.realistic_signal {
                    (rand_float() - 0.5) * 100.0
                } else {
                    let t = timestamp as f64 / 1_000_000.0;
                    let noise = (rand_float() - 0.5) * 20.0;
                    let alpha_strength = if ch == 4 { 15.0 } else { 5.0 };
                    let alpha = alpha_strength * (t * 10.0 * 2.0 * std::f64::consts::PI).sin() as f32;
                    (noise + alpha).clamp(-150.0, 150.0)
                }
            })
            .collect();
        
        Sample {
            device_timestamp: Some(timestamp),
            system_timestamp: timestamp,
            sequence_number: Some(self.sequence),
            values,
            quality: Some(vec![self.config.signal_quality; self.config.channel_count]),
        }
    }
}

impl Stream for MockSampleStream {
    type Item = Result<Sample>;
    
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Check if we should still be streaming
        if !self.streaming.load(Ordering::SeqCst) {
            return Poll::Ready(None);
        }
        
        // Wait for the next sample interval
        match self.interval.poll_tick(cx) {
            Poll::Ready(_) => {
                let sample = self.generate_sample();
                Poll::Ready(Some(Ok(sample)))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Simple pseudo-random float generator (for mock data, not cryptographic!)
fn rand_float() -> f32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(now_micros() as u64);
    (hasher.finish() % 10000) as f32 / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    
    #[tokio::test]
    async fn test_mock_provider_discovery() {
        let provider = MockProvider::new(MockDeviceConfig::default())
            .with_num_devices(3);
        
        let devices = provider.discover().await.unwrap();
        assert_eq!(devices.len(), 3);
        
        for (i, device) in devices.iter().enumerate() {
            assert_eq!(device.id.0, format!("mock_device_{}", i));
            assert!(matches!(device.device_type, DeviceType::Mock));
        }
    }
    
    #[tokio::test]
    async fn test_mock_device_streaming() {
        let provider = MockProvider::new(MockDeviceConfig::default());
        let devices = provider.discover().await.unwrap();
        
        let mut device = provider.connect(&devices[0].id, None).await.unwrap();
        assert!(device.is_connected());
        
        let mut stream = device.start_streaming().await.unwrap();
        assert!(device.is_streaming());
        
        // Collect a few samples
        let mut samples = Vec::new();
        for _ in 0..10 {
            if let Some(result) = stream.next().await {
                samples.push(result.unwrap());
            }
        }
        
        assert_eq!(samples.len(), 10);
        assert_eq!(samples[0].channel_count(), 5); // Default Insight-like config
    }
    
    #[tokio::test]
    async fn test_stop_streaming_terminates_stream() {
        let provider = MockProvider::new(MockDeviceConfig::default());
        let devices = provider.discover().await.unwrap();
        
        let mut device = provider.connect(&devices[0].id, None).await.unwrap();
        let mut stream = device.start_streaming().await.unwrap();
        
        // Grab a sample to confirm stream is alive
        assert!(stream.next().await.is_some());
        
        // Stop streaming — this must actually terminate the stream
        device.stop_streaming().await.unwrap();
        assert!(!device.is_streaming());
        
        // Stream should yield None (terminated) on next poll
        // Use a timeout to avoid hanging if the bug regresses
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            stream.next(),
        ).await;
        
        assert!(result.is_ok(), "stream should terminate promptly after stop_streaming");
        assert!(result.unwrap().is_none(), "stream should yield None after stop_streaming");
    }
}
