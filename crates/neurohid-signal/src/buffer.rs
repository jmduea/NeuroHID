//! # Sample Ring Buffer
//!
//! A circular buffer for storing multi-channel EEG samples with efficient
//! windowed access. Designed for the producer-consumer pattern where:
//!
//! - **Producer**: The pipeline pushes filtered samples as they arrive
//! - **Consumer**: Feature extraction reads overlapping windows
//!
//! ## Storage Format
//!
//! Samples are stored in columnar format (`channel_data[ch][time]`) rather
//! than row format. This makes per-channel operations (filtering, FFT) much
//! more cache-friendly since we access one channel's contiguous data at a time.

use std::collections::VecDeque;

use neurohid_types::error::SignalError;

/// Configuration for the sample buffer.
#[derive(Debug, Clone)]
pub struct BufferConfig {
    /// Maximum number of samples to retain. Older samples are discarded
    /// when this limit is reached. Should be at least as large as the
    /// largest feature extraction window.
    pub capacity_samples: usize,

    /// Number of channels. Must match the device configuration.
    pub channel_count: usize,
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            // ~8 seconds at 128Hz — enough for trailing averages and ErrP windows
            capacity_samples: 1024,
            channel_count: 5,
        }
    }
}

/// A windowed view into the buffer's data.
///
/// This is a snapshot — it owns copies of the data, so the buffer can continue
/// receiving samples while features are being extracted from this window.
#[derive(Debug, Clone)]
pub struct SignalWindow {
    /// Channel data in columnar format: `channel_data[ch][sample_idx]`.
    /// Sample index 0 is the oldest sample in the window.
    pub channel_data: Vec<Vec<f32>>,

    /// Timestamps corresponding to each sample (microseconds since epoch).
    pub timestamps: Vec<i64>,

    /// Number of channels.
    pub channel_count: usize,

    /// Number of samples in the window.
    pub sample_count: usize,
}

impl SignalWindow {
    /// Get data for a single channel as a slice.
    pub fn channel(&self, ch: usize) -> Option<&[f32]> {
        self.channel_data.get(ch).map(|v| v.as_slice())
    }

    /// Duration of this window in seconds.
    pub fn duration_secs(&self, sample_rate_hz: f32) -> f32 {
        self.sample_count as f32 / sample_rate_hz
    }

    /// Whether this window has enough data for a given minimum sample count.
    pub fn has_minimum(&self, min_samples: usize) -> bool {
        self.sample_count >= min_samples
    }
}

/// Circular buffer storing multi-channel signal data in columnar format.
///
/// Provides efficient windowed access for feature extraction while maintaining
/// a fixed memory footprint regardless of session duration.
pub struct SampleBuffer {
    /// Per-channel sample storage. Each deque holds one channel's time series.
    channel_data: Vec<VecDeque<f32>>,

    /// Timestamps for each sample position.
    timestamps: VecDeque<i64>,

    /// Buffer configuration.
    config: BufferConfig,

    /// Total samples pushed since creation (monotonically increasing).
    total_pushed: u64,
}

impl SampleBuffer {
    /// Create a new buffer with the given configuration.
    pub fn new(config: BufferConfig) -> Self {
        let channel_data = (0..config.channel_count)
            .map(|_| VecDeque::with_capacity(config.capacity_samples))
            .collect();

        Self {
            channel_data,
            timestamps: VecDeque::with_capacity(config.capacity_samples),
            config,
            total_pushed: 0,
        }
    }

    /// Push a filtered sample into the buffer.
    ///
    /// `values` must have exactly `channel_count` elements.
    /// If the buffer is at capacity, the oldest sample is evicted.
    pub fn push(&mut self, values: &[f32], timestamp: i64) -> Result<(), SignalError> {
        if values.len() != self.config.channel_count {
            return Err(SignalError::InvalidChannelConfig(format!(
                "expected {} channels, got {}",
                self.config.channel_count,
                values.len()
            )));
        }

        // Evict oldest sample if at capacity
        if self.timestamps.len() >= self.config.capacity_samples {
            self.timestamps.pop_front();
            for ch in &mut self.channel_data {
                ch.pop_front();
            }
        }

        // Push new data
        for (ch, &val) in self.channel_data.iter_mut().zip(values.iter()) {
            ch.push_back(val);
        }
        self.timestamps.push_back(timestamp);
        self.total_pushed += 1;

        Ok(())
    }

    /// Extract the most recent `num_samples` as a SignalWindow.
    ///
    /// Returns `None` if the buffer contains fewer than `num_samples`.
    pub fn window(&self, num_samples: usize) -> Option<SignalWindow> {
        let available = self.len();
        if available < num_samples {
            return None;
        }

        let start = available - num_samples;

        let channel_data: Vec<Vec<f32>> = self
            .channel_data
            .iter()
            .map(|ch| ch.range(start..).copied().collect())
            .collect();

        let timestamps: Vec<i64> = self.timestamps.range(start..).copied().collect();

        Some(SignalWindow {
            channel_data,
            timestamps,
            channel_count: self.config.channel_count,
            sample_count: num_samples,
        })
    }

    /// Number of samples currently in the buffer.
    pub fn len(&self) -> usize {
        self.timestamps.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.timestamps.is_empty()
    }

    /// Total number of samples pushed since creation.
    pub fn total_pushed(&self) -> u64 {
        self.total_pushed
    }

    /// Clear all stored data but retain the configuration.
    pub fn clear(&mut self) {
        for ch in &mut self.channel_data {
            ch.clear();
        }
        self.timestamps.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_window() {
        let mut buf = SampleBuffer::new(BufferConfig {
            capacity_samples: 10,
            channel_count: 2,
        });

        // Push 5 samples
        for i in 0..5 {
            buf.push(&[i as f32, (i * 10) as f32], i as i64)
                .unwrap();
        }

        assert_eq!(buf.len(), 5);

        let win = buf.window(3).unwrap();
        assert_eq!(win.sample_count, 3);
        assert_eq!(win.channel_data[0], vec![2.0, 3.0, 4.0]);
        assert_eq!(win.channel_data[1], vec![20.0, 30.0, 40.0]);
        assert_eq!(win.timestamps, vec![2, 3, 4]);
    }

    #[test]
    fn test_capacity_eviction() {
        let mut buf = SampleBuffer::new(BufferConfig {
            capacity_samples: 4,
            channel_count: 1,
        });

        for i in 0..8 {
            buf.push(&[i as f32], i as i64).unwrap();
        }

        assert_eq!(buf.len(), 4);
        assert_eq!(buf.total_pushed(), 8);

        let win = buf.window(4).unwrap();
        // Should contain the most recent 4 samples: 4, 5, 6, 7
        assert_eq!(win.channel_data[0], vec![4.0, 5.0, 6.0, 7.0]);
    }

    #[test]
    fn test_window_insufficient_data() {
        let mut buf = SampleBuffer::new(BufferConfig {
            capacity_samples: 10,
            channel_count: 1,
        });

        buf.push(&[1.0], 0).unwrap();
        assert!(buf.window(5).is_none());
    }

    #[test]
    fn test_wrong_channel_count() {
        let mut buf = SampleBuffer::new(BufferConfig {
            capacity_samples: 10,
            channel_count: 3,
        });

        let result = buf.push(&[1.0, 2.0], 0);
        assert!(result.is_err());
    }
}
