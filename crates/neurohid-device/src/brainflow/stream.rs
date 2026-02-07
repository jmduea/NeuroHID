//! # BrainFlow → Async Stream Adapter
//!
//! BrainFlow's data acquisition is pull-based and synchronous: you call
//! `board.get_current_board_data(n)` to retrieve the last N samples from
//! an internal ring buffer. Our `Device` trait expects an async `Stream`
//! that yields samples as they arrive.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────┐       channel        ┌──────────────────────┐
//! │  Polling Thread      │  ──────────────────▶ │  BrainFlowStream     │
//! │  (std::thread)       │   tx: Sample         │  (impl Stream)       │
//! │                      │                      │                      │
//! │  loop {              │                      │  poll_next() {       │
//! │    sleep(poll_ms)    │                      │    rx.try_recv()     │
//! │    get_board_data()  │                      │  }                   │
//! │    translate → Sample│                      │                      │
//! │    tx.send(sample)   │                      │                      │
//! │  }                   │                      │                      │
//! └─────────────────────┘                      └──────────────────────┘
//! ```
//!
//! The polling thread runs on a dedicated OS thread (not a Tokio task) because
//! BrainFlow's FFI calls are blocking and should not hold a Tokio worker thread.
//! We use a bounded `tokio::sync::mpsc` channel as the bridge.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

use brainflow::board_shim::BoardShim;
use brainflow::BoardIds;
use futures::Stream;
use tokio::sync::mpsc;

use neurohid_types::error::{DeviceError, Result};
use neurohid_types::now_micros;
use neurohid_types::signal::{ChannelConfig, ChannelId, DeviceChannelConfig, Sample};

/// Configuration for the polling adapter.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// How often to poll BrainFlow for new data (milliseconds).
    /// Should be significantly shorter than the sample period to avoid
    /// missing samples. Default: 4ms (catches most at 128-256 Hz).
    pub poll_interval_ms: u64,

    /// Maximum samples to retrieve per poll call.
    /// Prevents huge bursts if the consumer falls behind.
    pub max_samples_per_poll: usize,

    /// Channel buffer capacity. If the consumer can't keep up,
    /// oldest samples are dropped (bounded channel).
    pub channel_capacity: usize,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 4,
            max_samples_per_poll: 64,
            channel_capacity: 1024,
        }
    }
}

/// Metadata needed to translate BrainFlow's 2D array into `Sample` structs.
///
/// BrainFlow returns data as `Vec<Vec<f64>>` where rows can be EEG channels,
/// timestamps, markers, battery, etc. We need to know which rows are EEG
/// channels and which row is the timestamp.
#[derive(Debug, Clone)]
pub struct BoardChannelMap {
    /// Row indices in the BrainFlow data array that contain EEG data.
    pub eeg_row_indices: Vec<usize>,

    /// Row index for the timestamp column (device timestamp in Unix seconds).
    pub timestamp_row_index: usize,

    /// Row index for the package/sequence number, if available.
    pub package_num_row_index: Option<usize>,

    /// Channel names in 10-20 system (e.g., "Fp1", "Cz"), in EEG row order.
    pub channel_names: Vec<String>,

    /// Sampling rate in Hz.
    pub sampling_rate_hz: f32,
}

impl BoardChannelMap {
    /// Query BrainFlow for the channel layout of a specific board.
    ///
    /// This uses BrainFlow's static metadata methods which don't require
    /// an active board session.
    pub fn for_board(board_id: BoardIds) -> Result<Self> {
        let eeg_row_indices = BoardShim::get_eeg_channels(board_id as i32)
            .map_err(|e| {
                DeviceError::CommunicationError(format!(
                    "failed to get EEG channels for {:?}: {}",
                    board_id, e
                ))
            })?
            .iter()
            .map(|&x| x as usize)
            .collect();

        let timestamp_row_index =
            BoardShim::get_timestamp_channel(board_id as i32).map_err(|e| {
                DeviceError::CommunicationError(format!(
                    "failed to get timestamp channel for {:?}: {}",
                    board_id, e
                ))
            })? as usize;

        let package_num_row_index = BoardShim::get_package_num_channel(board_id as i32)
            .ok()
            .map(|x| x as usize);

        let channel_names = BoardShim::get_eeg_names(board_id as i32)
            .unwrap_or_default()
            .iter()
            .map(|s| s.to_string())
            .collect();

        let sampling_rate_hz = BoardShim::get_sampling_rate(board_id as i32).map_err(|e| {
            DeviceError::CommunicationError(format!(
                "failed to get sampling rate for {:?}: {}",
                board_id, e
            ))
        })? as f32;

        Ok(Self {
            eeg_row_indices,
            timestamp_row_index,
            package_num_row_index,
            channel_names,
            sampling_rate_hz,
        })
    }

    /// Build a `DeviceChannelConfig` from this channel map.
    pub fn to_channel_config(&self) -> DeviceChannelConfig {
        let channels: Vec<ChannelConfig> = self
            .channel_names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let name = if name.is_empty() {
                    format!("Ch{}", i)
                } else {
                    name.clone()
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
            sampling_rate_hz: self.sampling_rate_hz,
            resolution_bits: 24, // Most BrainFlow boards are 24-bit ADC
        }
    }

    /// Translate a single column from BrainFlow's 2D array into a `Sample`.
    ///
    /// `all_rows` is BrainFlow's full data array; `col` is the sample index
    /// (column) to extract.
    fn translate_sample(&self, all_rows: &[Vec<f64>], col: usize, sequence: u64) -> Sample {
        // Extract EEG values (in µV — BrainFlow already converts to µV)
        let values: Vec<f32> = self
            .eeg_row_indices
            .iter()
            .map(|&row| {
                all_rows
                    .get(row)
                    .and_then(|r| r.get(col))
                    .copied()
                    .unwrap_or(0.0) as f32
            })
            .collect();

        // Extract timestamp (BrainFlow uses Unix seconds as f64)
        let device_timestamp = all_rows
            .get(self.timestamp_row_index)
            .and_then(|r| r.get(col))
            .map(|&t| (t * 1_000_000.0) as i64); // Convert seconds → microseconds

        // Extract sequence number
        let seq_num = self.package_num_row_index.and_then(|row| {
            all_rows
                .get(row)
                .and_then(|r| r.get(col))
                .map(|&v| v as u64)
        });

        Sample {
            device_timestamp,
            system_timestamp: now_micros(),
            sequence_number: seq_num.or(Some(sequence)),
            values,
            quality: None, // BrainFlow doesn't provide per-channel quality
        }
    }
}

/// Async stream adapter that polls a BrainFlow board on a dedicated thread.
pub struct BrainFlowStream {
    rx: mpsc::Receiver<Result<Sample>>,
    alive: Arc<AtomicBool>,
}

impl BrainFlowStream {
    /// Start a polling thread that reads from the given BoardShim and sends
    /// samples through the returned stream.
    ///
    /// The caller must have already called `board.prepare_session()` and
    /// `board.start_stream()` before calling this.
    ///
    /// # Returns
    ///
    /// A `(BrainFlowStream, Arc<AtomicBool>)` pair. The stream yields samples;
    /// the `AtomicBool` can be set to `false` to stop the polling thread.
    pub fn start(
        board: Arc<BoardShim>,
        channel_map: BoardChannelMap,
        config: StreamConfig,
    ) -> (Self, Arc<AtomicBool>) {
        let alive = Arc::new(AtomicBool::new(true));
        let alive_clone = Arc::clone(&alive);
        let (tx, rx) = mpsc::channel(config.channel_capacity);

        let poll_interval = std::time::Duration::from_millis(config.poll_interval_ms);
        let max_per_poll = config.max_samples_per_poll;

        // Spawn dedicated OS thread for blocking BrainFlow calls
        std::thread::Builder::new()
            .name("brainflow-poll".into())
            .spawn(move || {
                let mut sequence: u64 = 0;

                while alive_clone.load(Ordering::Relaxed) {
                    std::thread::sleep(poll_interval);

                    // Pull new data from BrainFlow's internal ring buffer
                    let data = match board.get_current_board_data(max_per_poll as i32) {
                        Ok(d) => d,
                        Err(e) => {
                            let _ = tx.blocking_send(Err(DeviceError::CommunicationError(
                                format!("BrainFlow poll error: {}", e),
                            )
                            .into()));
                            continue;
                        }
                    };

                    // data is Vec<Vec<f64>>: rows × columns
                    // Number of new samples = length of any row (they're all the same)
                    let num_samples = data.first().map(|r| r.len()).unwrap_or(0);
                    if num_samples == 0 {
                        continue;
                    }

                    // Translate each column into a Sample
                    for col in 0..num_samples {
                        let sample = channel_map.translate_sample(&data, col, sequence);
                        sequence += 1;

                        if tx.blocking_send(Ok(sample)).is_err() {
                            // Receiver dropped — stream was closed
                            return;
                        }
                    }
                }

                tracing::debug!("BrainFlow polling thread exiting");
            })
            .expect("failed to spawn BrainFlow polling thread");

        let stream = Self {
            rx,
            alive: Arc::clone(&alive),
        };
        (stream, alive)
    }
}

impl Stream for BrainFlowStream {
    type Item = Result<Sample>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl Drop for BrainFlowStream {
    fn drop(&mut self) {
        // Signal the polling thread to stop
        self.alive.store(false, Ordering::Relaxed);
    }
}
