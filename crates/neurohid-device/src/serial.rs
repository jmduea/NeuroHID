//! Serial device backend — discovers serial ports and decodes sample frames.

use async_trait::async_trait;
use futures::Stream;
use serialport::{SerialPortInfo, SerialPortType};
use std::io::{ErrorKind, Read};
use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use tokio::sync::{mpsc, watch};

use neurohid_types::{
    config::{SerialConfig, SerialFraming},
    device::{ConnectionSettings, ConnectionState, DeviceId, DeviceInfo, DeviceStatus, DeviceType},
    error::{DeviceError, Result},
    now_micros,
    signal::{ChannelConfig, ChannelId, DeviceChannelConfig, Sample},
};

use crate::traits::{Device, DeviceProvider, SampleStream};

const SERIAL_TIMEOUT_MS: u64 = 100;
const DEFAULT_CSV_SAMPLING_RATE_HZ: f32 = 250.0;

type ParseResult = std::result::Result<Vec<f32>, DeviceError>;

/// Serial port provider with configurable framing/baud/channel mapping.
pub struct SerialProvider {
    config: SerialConfig,
}

impl SerialProvider {
    /// Create a new serial provider from configuration.
    pub fn new(config: SerialConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl DeviceProvider for SerialProvider {
    fn device_type(&self) -> DeviceType {
        DeviceType::Unknown("Serial".to_string())
    }

    async fn is_available(&self) -> bool {
        if self.config.port.is_some() {
            return true;
        }

        tokio::task::spawn_blocking(|| serialport::available_ports().map(|ports| !ports.is_empty()))
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or(false)
    }

    async fn discover(&self) -> Result<Vec<DeviceInfo>> {
        let configured_port = self.config.port.clone();
        let serial_cfg = self.config.clone();

        let devices = tokio::task::spawn_blocking(move || {
            discover_serial_ports(configured_port, &serial_cfg)
        })
        .await
        .map_err(|e| {
            DeviceError::CommunicationError(format!("Serial discovery task panicked: {e}"))
        })??;

        Ok(devices)
    }

    async fn connect(
        &self,
        device_id: &DeviceId,
        _settings: Option<ConnectionSettings>,
    ) -> Result<Box<dyn Device>> {
        let requested = normalize_port_id(&device_id.0);
        let port_path = if !requested.is_empty() {
            requested
        } else if let Some(configured) = self.config.port.clone() {
            configured
        } else {
            return Err(DeviceError::NoDeviceFound.into());
        };

        let baud_rate = self.config.baud_rate;
        let open_port = port_path.clone();
        tokio::task::spawn_blocking(move || {
            serialport::new(&open_port, baud_rate)
                .timeout(std::time::Duration::from_millis(SERIAL_TIMEOUT_MS))
                .open()
                .map(|_| ())
                .map_err(|e| DeviceError::ConnectionFailed {
                    reason: format!("Failed to open serial port '{open_port}': {e}"),
                })
        })
        .await
        .map_err(|e| {
            DeviceError::CommunicationError(format!("Serial connect task panicked: {e}"))
        })??;

        let device_info = device_info_for_port(&port_path, None, &self.config);
        let device = SerialDevice::new(port_path, self.config.clone(), device_info);
        Ok(Box::new(device))
    }
}

fn discover_serial_ports(
    configured_port: Option<String>,
    config: &SerialConfig,
) -> std::result::Result<Vec<DeviceInfo>, DeviceError> {
    let mut devices = Vec::new();

    match serialport::available_ports() {
        Ok(ports) => {
            for port in ports {
                devices.push(device_info_from_port_info(&port, config));
            }
        }
        Err(e) => {
            if configured_port.is_none() {
                return Err(DeviceError::CommunicationError(format!(
                    "Serial discovery failed: {e}"
                )));
            }
        }
    }

    if let Some(port) = configured_port {
        let configured_id = device_id_for_port(&port);
        if !devices.iter().any(|d| d.id == configured_id) {
            devices.push(device_info_for_port(&port, Some("Configured Port"), config));
        }
    }

    Ok(devices)
}

fn device_info_from_port_info(info: &SerialPortInfo, config: &SerialConfig) -> DeviceInfo {
    let label = port_label(info);
    device_info_for_port(&info.port_name, Some(&label), config)
}

fn device_info_for_port(port_name: &str, label: Option<&str>, config: &SerialConfig) -> DeviceInfo {
    let channel_config = channel_config(config);
    DeviceInfo {
        id: device_id_for_port(port_name),
        device_type: DeviceType::Unknown("Serial".to_string()),
        name: Some(label.unwrap_or(port_name).to_string()),
        firmware_version: None,
        channel_config: Some(channel_config),
        battery_percent: None,
        source_id: Some(format!("serial:{port_name}")),
    }
}

fn device_id_for_port(port_name: &str) -> DeviceId {
    DeviceId::new(format!("serial::{port_name}"))
}

fn normalize_port_id(device_id: &str) -> String {
    device_id
        .strip_prefix("serial::")
        .unwrap_or(device_id)
        .trim()
        .to_string()
}

fn channel_config(config: &SerialConfig) -> DeviceChannelConfig {
    let channel_count = config.channels.max(1);
    let channels: Vec<ChannelConfig> = (0..channel_count)
        .map(|idx| {
            let name = format!("Ch{idx}");
            ChannelConfig {
                id: ChannelId::new(&name),
                position_10_20: None,
                enabled: true,
                reference: None,
            }
        })
        .collect();

    DeviceChannelConfig {
        channels,
        sampling_rate_hz: estimated_sampling_rate_hz(config),
        resolution_bits: 16,
    }
}

fn estimated_sampling_rate_hz(config: &SerialConfig) -> f32 {
    match config.framing {
        SerialFraming::BinaryI16Le => {
            let bytes_per_sample = (config.channels.max(1) * 2) as f32;
            ((config.baud_rate as f32 / 10.0) / bytes_per_sample).max(1.0)
        }
        SerialFraming::CsvLine => DEFAULT_CSV_SAMPLING_RATE_HZ,
    }
}

fn port_label(info: &SerialPortInfo) -> String {
    match &info.port_type {
        SerialPortType::UsbPort(usb) => {
            let mut details = Vec::new();
            if let Some(product) = usb.product.as_ref() {
                details.push(product.clone());
            }
            if let Some(manufacturer) = usb.manufacturer.as_ref() {
                details.push(manufacturer.clone());
            }
            if details.is_empty() {
                info.port_name.clone()
            } else {
                format!("{} ({})", info.port_name, details.join(", "))
            }
        }
        _ => info.port_name.clone(),
    }
}

/// Connected serial adapter device.
pub struct SerialDevice {
    id: DeviceId,
    info: DeviceInfo,
    config: SerialConfig,
    channel_config: DeviceChannelConfig,
    port_path: String,
    connected: AtomicBool,
    streaming: Arc<AtomicBool>,
    samples_received: Arc<AtomicU64>,
    status_tx: watch::Sender<DeviceStatus>,
    status_rx: watch::Receiver<DeviceStatus>,
}

impl SerialDevice {
    fn new(port_path: String, config: SerialConfig, info: DeviceInfo) -> Self {
        let channel_config = info
            .channel_config
            .clone()
            .unwrap_or_else(|| channel_config(&config));
        let device_id = info.id.clone();
        let initial = DeviceStatus {
            device_id: device_id.clone(),
            connection_state: ConnectionState::Connected,
            is_streaming: false,
            samples_received: 0,
            samples_dropped: 0,
            battery_percent: None,
            channel_quality: None,
            message: Some(format!("Connected to serial port '{port_path}'")),
        };
        let (status_tx, status_rx) = watch::channel(initial);

        Self {
            id: device_id,
            info,
            config,
            channel_config,
            port_path,
            connected: AtomicBool::new(true),
            streaming: Arc::new(AtomicBool::new(false)),
            samples_received: Arc::new(AtomicU64::new(0)),
            status_tx,
            status_rx,
        }
    }

    fn update_status(&self, message: Option<String>) {
        let status = DeviceStatus {
            device_id: self.id.clone(),
            connection_state: if self.connected.load(Ordering::SeqCst) {
                ConnectionState::Connected
            } else {
                ConnectionState::Disconnected
            },
            is_streaming: self.streaming.load(Ordering::SeqCst),
            samples_received: self.samples_received.load(Ordering::SeqCst),
            samples_dropped: 0,
            battery_percent: None,
            channel_quality: None,
            message,
        };
        let _ = self.status_tx.send(status);
    }
}

#[async_trait]
impl Device for SerialDevice {
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
        let mut status = self.status_rx.borrow().clone();
        status.is_streaming = self.streaming.load(Ordering::SeqCst);
        status.samples_received = self.samples_received.load(Ordering::SeqCst);
        status.connection_state = if self.connected.load(Ordering::SeqCst) {
            ConnectionState::Connected
        } else {
            ConnectionState::Disconnected
        };
        status
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn is_streaming(&self) -> bool {
        self.streaming.load(Ordering::SeqCst)
    }

    async fn start_streaming(&mut self) -> Result<SampleStream> {
        if !self.is_connected() {
            return Err(DeviceError::NotConnected.into());
        }
        if self.is_streaming() {
            return Err(DeviceError::DeviceBusy.into());
        }

        self.streaming.store(true, Ordering::SeqCst);
        self.update_status(Some("Serial stream started".to_string()));

        let port_path = self.port_path.clone();
        let baud_rate = self.config.baud_rate;
        let framing = self.config.framing.clone();
        let channels = self.config.channels.max(1);
        let device_source = self.id.0.clone();
        let streaming = Arc::clone(&self.streaming);
        let samples_received = Arc::clone(&self.samples_received);

        let (tx, rx) = mpsc::channel::<Result<Sample>>(1024);

        tokio::task::spawn_blocking(move || {
            let mut port = match serialport::new(&port_path, baud_rate)
                .timeout(std::time::Duration::from_millis(SERIAL_TIMEOUT_MS))
                .open()
            {
                Ok(port) => port,
                Err(e) => {
                    let _ = tx.blocking_send(Err(DeviceError::ConnectionFailed {
                        reason: format!("Failed to open serial port '{port_path}': {e}"),
                    }
                    .into()));
                    streaming.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let mut decoder = SerialFrameDecoder::new(framing, channels);
            let mut buffer = [0_u8; 4096];
            let mut sequence_number = 0_u64;
            let mut malformed_frames = 0_u64;

            while streaming.load(Ordering::Relaxed) {
                match port.read(&mut buffer) {
                    Ok(0) => continue,
                    Ok(n) => {
                        for parsed in decoder.push_and_decode(&buffer[..n]) {
                            match parsed {
                                Ok(values) => {
                                    sequence_number += 1;
                                    samples_received.fetch_add(1, Ordering::Relaxed);
                                    let sample = Sample {
                                        source_id: Some(device_source.clone()),
                                        device_timestamp: None,
                                        system_timestamp: now_micros(),
                                        sequence_number: Some(sequence_number),
                                        values,
                                        quality: None,
                                    };
                                    if tx.blocking_send(Ok(sample)).is_err() {
                                        streaming.store(false, Ordering::SeqCst);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    malformed_frames += 1;
                                    if malformed_frames == 1 || malformed_frames.is_multiple_of(100)
                                    {
                                        tracing::warn!(
                                            "Serial decoder dropped malformed frame (count={}): {}",
                                            malformed_frames,
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::TimedOut => continue,
                    Err(e) => {
                        let _ = tx.blocking_send(Err(DeviceError::CommunicationError(format!(
                            "Serial read error on '{port_path}': {e}"
                        ))
                        .into()));
                        break;
                    }
                }
            }

            streaming.store(false, Ordering::SeqCst);
            tracing::info!(
                "Serial reader stopped for '{}' after {} samples",
                port_path,
                sequence_number
            );
        });

        let stream = futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        });

        Ok(Box::pin(stream))
    }

    async fn stop_streaming(&mut self) -> Result<()> {
        self.streaming.store(false, Ordering::SeqCst);
        self.update_status(Some("Serial stream stopped".to_string()));
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.streaming.store(false, Ordering::SeqCst);
        self.connected.store(false, Ordering::SeqCst);
        self.update_status(Some("Serial device disconnected".to_string()));
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

impl Drop for SerialDevice {
    fn drop(&mut self) {
        self.streaming.store(false, Ordering::SeqCst);
    }
}

enum SerialFrameDecoder {
    Csv(CsvLineDecoder),
    Binary(BinaryI16LeDecoder),
}

impl SerialFrameDecoder {
    fn new(framing: SerialFraming, channels: usize) -> Self {
        let channels = channels.max(1);
        match framing {
            SerialFraming::CsvLine => Self::Csv(CsvLineDecoder::new(channels)),
            SerialFraming::BinaryI16Le => Self::Binary(BinaryI16LeDecoder::new(channels)),
        }
    }

    fn push_and_decode(&mut self, bytes: &[u8]) -> Vec<ParseResult> {
        match self {
            SerialFrameDecoder::Csv(decoder) => decoder.push_and_decode(bytes),
            SerialFrameDecoder::Binary(decoder) => decoder.push_and_decode(bytes),
        }
    }
}

#[derive(Debug)]
struct CsvLineDecoder {
    channels: usize,
    pending: Vec<u8>,
}

impl CsvLineDecoder {
    fn new(channels: usize) -> Self {
        Self {
            channels: channels.max(1),
            pending: Vec::new(),
        }
    }

    fn push_and_decode(&mut self, bytes: &[u8]) -> Vec<ParseResult> {
        self.pending.extend_from_slice(bytes);
        let mut decoded = Vec::new();

        while let Some(newline_pos) = self.pending.iter().position(|b| *b == b'\n') {
            let line_with_newline: Vec<u8> = self.pending.drain(..=newline_pos).collect();
            let line = trim_newline_bytes(&line_with_newline);
            if line.is_empty() {
                continue;
            }

            let utf8 = match std::str::from_utf8(line) {
                Ok(line) => line,
                Err(e) => {
                    decoded.push(Err(DeviceError::InvalidData(format!(
                        "CSV frame is not valid UTF-8: {e}"
                    ))));
                    continue;
                }
            };

            decoded.push(parse_csv_line(utf8, self.channels));
        }

        decoded
    }
}

#[derive(Debug)]
struct BinaryI16LeDecoder {
    channels: usize,
    pending: Vec<u8>,
}

impl BinaryI16LeDecoder {
    fn new(channels: usize) -> Self {
        Self {
            channels: channels.max(1),
            pending: Vec::new(),
        }
    }

    fn push_and_decode(&mut self, bytes: &[u8]) -> Vec<ParseResult> {
        self.pending.extend_from_slice(bytes);
        let mut decoded = Vec::new();
        let frame_len = self.channels * 2;

        while self.pending.len() >= frame_len {
            let frame: Vec<u8> = self.pending.drain(..frame_len).collect();
            decoded.push(parse_binary_i16_le_frame(&frame, self.channels));
        }

        decoded
    }
}

fn trim_newline_bytes(line: &[u8]) -> &[u8] {
    if line.ends_with(b"\r\n") {
        &line[..line.len() - 2]
    } else if line.ends_with(b"\n") || line.ends_with(b"\r") {
        &line[..line.len() - 1]
    } else {
        line
    }
}

fn parse_csv_line(line: &str, expected_channels: usize) -> ParseResult {
    if expected_channels == 0 {
        return Err(DeviceError::InvalidData(
            "Expected channel count cannot be zero".to_string(),
        ));
    }

    let values: std::result::Result<Vec<f32>, _> = line
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| {
            token.parse::<f32>().map_err(|e| {
                DeviceError::InvalidData(format!("CSV token '{token}' is not a float: {e}"))
            })
        })
        .collect();

    let values = values?;
    if values.len() != expected_channels {
        return Err(DeviceError::InvalidData(format!(
            "CSV channel count mismatch: expected {}, got {}",
            expected_channels,
            values.len()
        )));
    }

    Ok(values)
}

fn parse_binary_i16_le_frame(frame: &[u8], channels: usize) -> ParseResult {
    if channels == 0 {
        return Err(DeviceError::InvalidData(
            "Expected channel count cannot be zero".to_string(),
        ));
    }
    let expected_len = channels * 2;
    if frame.len() != expected_len {
        return Err(DeviceError::InvalidData(format!(
            "Binary frame size mismatch: expected {} bytes, got {}",
            expected_len,
            frame.len()
        )));
    }

    let mut values = Vec::with_capacity(channels);
    for bytes in frame.chunks_exact(2) {
        let raw = i16::from_le_bytes([bytes[0], bytes[1]]);
        values.push(raw as f32);
    }

    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_parser_rejects_non_numeric_tokens() {
        let err = parse_csv_line("1.0,abc,3.0", 3).unwrap_err();
        assert!(matches!(err, DeviceError::InvalidData(_)));
    }

    #[test]
    fn csv_parser_rejects_wrong_channel_count() {
        let err = parse_csv_line("1.0,2.0", 3).unwrap_err();
        assert!(matches!(err, DeviceError::InvalidData(_)));
    }

    #[test]
    fn csv_parser_accepts_valid_row() {
        let values = parse_csv_line("1.25, -2.5, 3.75", 3).unwrap();
        assert_eq!(values, vec![1.25, -2.5, 3.75]);
    }

    #[test]
    fn binary_parser_handles_fragmented_frames() {
        let mut decoder = BinaryI16LeDecoder::new(2);
        let first = decoder.push_and_decode(&[0x01, 0x00, 0xff]);
        assert!(first.is_empty());

        let second = decoder.push_and_decode(&[0x7f, 0x00, 0x80, 0x00, 0x00]);
        assert_eq!(second.len(), 2);
        assert_eq!(second[0].as_ref().unwrap(), &vec![1.0, 32767.0]);
        assert_eq!(second[1].as_ref().unwrap(), &vec![-32768.0, 0.0]);
    }

    #[test]
    fn binary_parser_rejects_wrong_frame_size() {
        let err = parse_binary_i16_le_frame(&[0x01, 0x00, 0x02], 2).unwrap_err();
        assert!(matches!(err, DeviceError::InvalidData(_)));
    }
}
