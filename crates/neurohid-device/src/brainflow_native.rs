//! Native BrainFlow Device and SampleStream using the real BrainFlow SDK.
//!
//! Compiled only when `brainflow-native` is enabled. Maps BoardShim data to
//! NeuroHID `Sample` and the same `Device` / `SampleStream` pipeline as LSL.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::{Context, Poll};

use async_trait::async_trait;
use brainflow::{
    BoardIds, BrainFlowInputParams, BrainFlowPresets,
    board_shim::{self, eeg_channels, package_num_channel, timestamp_channel},
    brainflow_input_params::BrainFlowInputParamsBuilder,
};
use futures::Stream;
use ndarray::Array2;
use neurohid_types::{
    device::{DeviceId, DeviceInfo, DeviceStatus, DeviceType},
    error::{DeviceError, Result},
    signal::{ChannelConfig, ChannelId, DeviceChannelConfig, Sample},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::instrument;

use crate::traits::{Device, SampleStream};

const STREAM_BUFFER_SIZE: usize = 45_000;
const POLL_SAMPLES: usize = 64;

/// Map NeuroHID board_id (0 = synthetic, 1 = Cyton, 2 = Ganglion, ...) to BrainFlow BoardIds.
/// BrainFlow: SyntheticBoard = -1, CytonBoard = 0, GanglionBoard = 1.
fn to_brainflow_board_id(board_id: i32) -> BoardIds {
    match board_id {
        0 => BoardIds::SyntheticBoard,
        n => num::FromPrimitive::from_i32(n - 1).unwrap_or(BoardIds::SyntheticBoard),
    }
}

/// Build BrainFlowInputParams from config (serial_port, etc.).
fn to_input_params(serial_port: Option<&str>) -> BrainFlowInputParams {
    let mut b = BrainFlowInputParamsBuilder::default();
    if let Some(port) = serial_port.filter(|s| !s.is_empty()) {
        b = b.serial_port(port);
    }
    b.build()
}

/// Convert one column of BrainFlow data (num_rows x num_samples) to a Sample.
/// Rows: timestamp_channel, eeg_channels, package_num_channel (if available).
fn board_data_column_to_sample(
    data: &Array2<f64>,
    col: usize,
    ts_row: usize,
    eeg_rows: &[usize],
    pkg_row: Option<usize>,
    source_id: Option<String>,
) -> Sample {
    let device_ts = data.get((ts_row, col)).copied().map(|t| t as i64);
    let values: Vec<f32> = eeg_rows
        .iter()
        .filter_map(|&r| data.get((r, col)).copied())
        .map(|v| v as f32)
        .collect();
    let seq = pkg_row.and_then(|r| data.get((r, col)).copied().map(|n| n as u64));
    Sample {
        source_id: source_id.clone(),
        device_timestamp: device_ts,
        system_timestamp: neurohid_types::now_micros(),
        sequence_number: seq,
        values,
        quality: None,
    }
}

/// Native BrainFlow device: holds session and streams via get_board_data → Sample.
pub struct BrainFlowNativeDevice {
    board_shim: Arc<Mutex<brainflow::board_shim::BoardShim>>,
    board_id: BoardIds,
    preset: BrainFlowPresets,
    info: DeviceInfo,
    channel_config: DeviceChannelConfig,
    source_id: Option<String>,
    cancel: CancellationToken,
    streaming: std::sync::atomic::AtomicBool,
}

impl BrainFlowNativeDevice {
    /// Create device from an already-prepared BoardShim (prepare_session already called).
    pub fn new(
        board_shim: brainflow::board_shim::BoardShim,
        board_id: BoardIds,
        preset: BrainFlowPresets,
        info: DeviceInfo,
        channel_config: DeviceChannelConfig,
        source_id: Option<String>,
    ) -> Self {
        Self {
            board_shim: Arc::new(Mutex::new(board_shim)),
            board_id,
            preset,
            info,
            channel_config,
            source_id,
            cancel: CancellationToken::new(),
            streaming: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl Device for BrainFlowNativeDevice {
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
        DeviceStatus {
            connection_state: neurohid_types::device::ConnectionState::Connected,
            streaming: self.streaming.load(std::sync::atomic::Ordering::Relaxed),
            battery_percent: None,
            channel_quality: None,
        }
    }

    fn is_connected(&self) -> bool {
        true
    }

    fn is_streaming(&self) -> bool {
        self.streaming.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[instrument(skip(self), level = "debug")]
    async fn start_streaming(&mut self) -> Result<SampleStream> {
        let board = Arc::clone(&self.board_shim);
        {
            let b = board.lock().map_err(|_| DeviceError::ConnectionLost)?;
            board_shim::BoardShim::start_stream(&b, STREAM_BUFFER_SIZE, "")?;
        }
        self.streaming
            .store(true, std::sync::atomic::Ordering::Relaxed);

        let (tx, mut rx) = mpsc::channel::<Result<Sample>>(256);
        let cancel = self.cancel.clone();
        let board_id = self.board_id;
        let preset = self.preset;
        let source_id = self.source_id.clone();

        let ts_row = timestamp_channel(board_id, preset).map_err(|e| {
            neurohid_types::error::Error::Device(DeviceError::Other(anyhow::anyhow!(
                "timestamp_channel: {}",
                e
            )))
        })?;
        let eeg_rows = eeg_channels(board_id, preset).map_err(|e| {
            neurohid_types::error::Error::Device(DeviceError::Other(anyhow::anyhow!(
                "eeg_channels: {}",
                e
            )))
        })?;
        let pkg_row = package_num_channel(board_id, preset).ok();

        tokio::task::spawn_blocking(move || {
            loop {
                if cancel.is_cancelled() {
                    break;
                }
                let res = {
                    let b = match board.lock() {
                        Ok(guard) => guard,
                        Err(_) => break,
                    };
                    b.get_board_data(Some(POLL_SAMPLES), preset)
                };
                match res {
                    Ok(data) => {
                        let n_cols = data.ncols();
                        if n_cols == 0 {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                            continue;
                        }
                        for col in 0..n_cols {
                            let sample = board_data_column_to_sample(
                                &data,
                                col,
                                ts_row,
                                &eeg_rows,
                                pkg_row,
                                source_id.clone(),
                            );
                            if tx.blocking_send(Ok(sample)).is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.blocking_send(Err(neurohid_types::error::Error::Device(
                            DeviceError::Other(anyhow::anyhow!("get_board_data: {}", e)),
                        )));
                    }
                }
            }
            if let Ok(b) = board.lock() {
                let _ = board_shim::BoardShim::stop_stream(&b);
                let _ = board_shim::BoardShim::release_session(&b);
            }
        });

        struct NativeSampleStream(mpsc::Receiver<Result<Sample>>);
        impl Stream for NativeSampleStream {
            type Item = Result<Sample>;
            fn poll_next(
                mut self: Pin<&mut Self>,
                cx: &mut Context<'_>,
            ) -> Poll<Option<Result<Sample>>> {
                self.0.poll_recv(cx)
            }
        }
        Ok(Box::pin(NativeSampleStream(rx)))
    }

    async fn stop_streaming(&mut self) -> Result<()> {
        self.cancel.cancel();
        self.streaming
            .store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.cancel.cancel();
        self.streaming
            .store(false, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    fn status_stream(&self) -> std::pin::Pin<Box<dyn Stream<Item = DeviceStatus> + Send>> {
        let status = self.status();
        Box::pin(futures::stream::once(async move { status }))
    }
}

/// Match brainflow.rs token normalization so device IDs match discover().
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

/// Build DeviceInfo and channel config from BrainFlow board_id and optional serial port.
pub fn native_device_metadata(
    board_id: i32,
    serial_port: Option<&str>,
) -> Result<(DeviceInfo, DeviceChannelConfig)> {
    let bf_id = to_brainflow_board_id(board_id);
    let preset = BrainFlowPresets::DefaultPreset;

    let channel_count = board_shim::eeg_channels(bf_id, preset)
        .map(|v| v.len())
        .unwrap_or(8);
    let sampling_rate_hz = board_shim::sampling_rate(bf_id, preset).unwrap_or(250) as f32;

    let channels: Vec<ChannelConfig> = (0..channel_count)
        .map(|i| {
            let name = format!("EEG{}", i + 1);
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
        sampling_rate_hz,
        resolution_bits: 24,
    };

    let board_token = normalize_token(&board_id.to_string());
    let port_token = normalize_token(serial_port.unwrap_or("auto"));
    let id = DeviceId::new(format!("brainflow::{}::{}", board_token, port_token));
    let source_id = Some(format!("brainflow:{}:{}", board_token, port_token));
    let name = serial_port
        .map(|p| format!("BrainFlow Board {} ({})", board_id, p))
        .unwrap_or_else(|| format!("BrainFlow Board {}", board_id));

    let device_type = match board_id {
        1 => DeviceType::OpenBCICyton,
        2 => DeviceType::OpenBCIGanglion,
        _ => DeviceType::Unknown(format!("BrainFlow/Board-{}", board_id)),
    };

    let info = DeviceInfo {
        id,
        device_type,
        name: Some(name),
        firmware_version: None,
        channel_config: Some(channel_config.clone()),
        battery_percent: None,
        source_id: source_id.clone(),
    };

    Ok((info, channel_config))
}

/// Connect to a native BrainFlow board: prepare_session and return a native Device.
pub fn connect_native(
    board_id: i32,
    serial_port: Option<&str>,
    device_id: &DeviceId,
) -> Result<BrainFlowNativeDevice> {
    let bf_id = to_brainflow_board_id(board_id);
    let params = to_input_params(serial_port);
    let board = board_shim::BoardShim::new(bf_id, params)
        .map_err(|e| DeviceError::Other(anyhow::anyhow!("BoardShim::new: {}", e)))?;
    board
        .prepare_session()
        .map_err(|e| DeviceError::Other(anyhow::anyhow!("prepare_session: {}", e)))?;

    let (info, channel_config) = native_device_metadata(board_id, serial_port)?;
    if info.id != *device_id {
        let _ = board.release_session();
        return Err(DeviceError::NoDeviceFound.into());
    }

    let source_id = info.source_id.clone();
    Ok(BrainFlowNativeDevice::new(
        board,
        bf_id,
        BrainFlowPresets::DefaultPreset,
        info,
        channel_config,
        source_id,
    ))
}
