//! # Signal Task
//!
//! This task sits between the device and the decoder. It receives raw EEG
//! samples, applies digital filters to clean up the signal, and extracts
//! features that the neural network can understand.
//!
//! Think of it like a translator: the device speaks in raw voltage readings,
//! but the decoder needs higher-level summaries like "how much alpha rhythm
//! is present right now?" The signal task does that translation.
//!
//! Supports multiple concurrent streams — each stream gets its own sample
//! buffer and independent feature extraction. Features from all streams are
//! merged into a single output channel.

use neurohid_signal::{
    FeatureConfig as DspFeatureConfig, FeatureExtractor as DspFeatureExtractor, SignalWindow,
};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

use neurohid_types::{
    config::SignalConfig,
    error::Result,
    event::{MarkerPayload, MarkerType, StreamMarker},
    reward::ErrPResult,
    signal::{FeatureVector, Sample},
};

use crate::service::{ServiceState, SignalCommand};
use crate::tasks::latency::RollingLatency;

/// Default key for samples without a source_id (e.g., mock device).
const DEFAULT_STREAM_KEY: &str = "__default__";

/// Per-stream processing state.
struct StreamBuffer {
    samples: VecDeque<Sample>,
    samples_since_extraction: usize,
    estimated_sample_rate_hz: f32,
    last_sample_timestamp_micros: Option<i64>,
}

impl StreamBuffer {
    fn new(buffer_capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(buffer_capacity.max(1)),
            samples_since_extraction: 0,
            estimated_sample_rate_hz: 128.0,
            last_sample_timestamp_micros: None,
        }
    }

    fn update_sample_rate_estimate(&mut self, sample: &Sample) {
        let timestamp = sample.device_timestamp.unwrap_or(sample.system_timestamp);

        if let Some(previous_timestamp) = self.last_sample_timestamp_micros {
            let delta_micros = timestamp.saturating_sub(previous_timestamp);
            if delta_micros > 0 {
                let instantaneous_rate_hz = 1_000_000.0 / delta_micros as f32;
                if instantaneous_rate_hz.is_finite()
                    && (8.0..=2048.0).contains(&instantaneous_rate_hz)
                {
                    self.estimated_sample_rate_hz =
                        self.estimated_sample_rate_hz * 0.9 + instantaneous_rate_hz * 0.1;
                }
            }
        }

        self.last_sample_timestamp_micros = Some(timestamp);
    }
}

/// The signal processing task.
pub struct SignalTask {
    config: SignalConfig,
    sample_rx: mpsc::Receiver<Sample>,
    feature_tx: mpsc::Sender<FeatureVector>,
    errp_rx: mpsc::Receiver<ErrPResult>,
    state: Arc<RwLock<ServiceState>>,
    signal_command_rx: Option<mpsc::Receiver<SignalCommand>>,
    sample_tap_tx: Option<mpsc::Sender<Sample>>,

    /// Broadcast channel for forwarding raw samples to hub visualization widgets.
    sample_broadcast_tx: Option<broadcast::Sender<Sample>>,
    /// Broadcast channel for forwarding features to hub visualization widgets.
    feature_broadcast_tx: Option<broadcast::Sender<FeatureVector>>,
    /// Broadcast channel for forwarding timeline markers.
    marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,

    // Internal state for signal processing
    stream_buffers: HashMap<String, StreamBuffer>,
    sample_count: u64,
    last_head_movement_marker_us: i64,
    head_movement_threshold: f32,
    signal_latency: RollingLatency,
}

impl SignalTask {
    fn samples_for_duration_ms(duration_ms: u32, sample_rate_hz: f32) -> usize {
        let duration_secs = duration_ms.max(1) as f32 / 1000.0;
        let sample_rate_hz = if sample_rate_hz.is_finite() {
            sample_rate_hz.clamp(8.0, 2048.0)
        } else {
            128.0
        };

        (duration_secs * sample_rate_hz).round().max(1.0) as usize
    }

    /// Choose a Welch segment length that is valid for the current window.
    ///
    /// Uses the largest power-of-two <= `sample_count`, capped at 256.
    fn welch_segment_len_for_window(sample_count: usize) -> usize {
        if sample_count <= 1 {
            return sample_count.max(1);
        }

        let max_allowed = sample_count.min(256);
        let mut segment_len = 1usize;
        while segment_len.saturating_mul(2) <= max_allowed {
            segment_len *= 2;
        }
        segment_len
    }

    /// Creates a new signal task.
    pub fn new(
        config: SignalConfig,
        sample_rx: mpsc::Receiver<Sample>,
        feature_tx: mpsc::Sender<FeatureVector>,
        errp_rx: mpsc::Receiver<ErrPResult>,
        state: Arc<RwLock<ServiceState>>,
        signal_command_rx: Option<mpsc::Receiver<SignalCommand>>,
        sample_tap_tx: Option<mpsc::Sender<Sample>>,
        sample_broadcast_tx: Option<broadcast::Sender<Sample>>,
        feature_broadcast_tx: Option<broadcast::Sender<FeatureVector>>,
        marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,
    ) -> Self {
        Self {
            config,
            sample_rx,
            feature_tx,
            errp_rx,
            state,
            signal_command_rx,
            sample_tap_tx,
            sample_broadcast_tx,
            feature_broadcast_tx,
            marker_broadcast_tx,
            stream_buffers: HashMap::new(),
            sample_count: 0,
            last_head_movement_marker_us: 0,
            head_movement_threshold: 0.4,
            signal_latency: RollingLatency::new(512),
        }
    }

    /// Runs the signal task until shutdown is signaled.
    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        tracing::info!("Signal processing task started");

        loop {
            // Apply all pending runtime config updates without blocking the stream.
            if let Some(rx) = &mut self.signal_command_rx {
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        SignalCommand::UpdateConfig(cfg) => {
                            self.config = cfg;
                            tracing::info!("SignalTask config updated at runtime");
                        }
                    }
                }
            }

            tokio::select! {
                // Check for shutdown
                _ = shutdown.recv() => {
                    tracing::info!("Signal task received shutdown signal");
                    break;
                }

                // Receive samples from device task (any stream)
                sample = self.sample_rx.recv() => {
                    match sample {
                        Some(sample) => {
                            self.sample_count += 1;

                            if let Some(tx) = &self.sample_tap_tx {
                                let _ = tx.try_send(sample.clone());
                            }

                            // Broadcast raw sample to hub visualization widgets
                            if let Some(tx) = &self.sample_broadcast_tx {
                                let _ = tx.send(sample.clone());
                            }

                            self.maybe_emit_head_movement_marker(&sample);

                            // Route sample to the correct per-stream buffer
                            let stream_key = sample.source_id.clone()
                                .unwrap_or_else(|| DEFAULT_STREAM_KEY.to_string());

                            let buf = self.stream_buffers
                                .entry(stream_key)
                                .or_insert_with(|| StreamBuffer::new(self.config.buffer_size_samples));

                            buf.update_sample_rate_estimate(&sample);

                            let samples_per_window = Self::samples_for_duration_ms(
                                self.config.feature_window_ms,
                                buf.estimated_sample_rate_hz,
                            );
                            let samples_per_step = Self::samples_for_duration_ms(
                                self.config.feature_step_ms,
                                buf.estimated_sample_rate_hz,
                            );

                            buf.samples.push_back(sample);
                            buf.samples_since_extraction += 1;

                            // Keep buffer from growing unbounded
                            while buf.samples.len() > self.config.buffer_size_samples {
                                let _ = buf.samples.pop_front();
                            }

                            // Check if it's time to extract features for this stream
                            if buf.samples_since_extraction >= samples_per_step
                                && buf.samples.len() >= samples_per_window
                            {
                                buf.samples_since_extraction = 0;

                                // Extract features from the most recent window.
                                // Capture the newest sample timestamp so we can track
                                // signal-stage latency (sample -> feature output).
                                let (features, newest_sample_timestamp) = {
                                    let samples = buf.samples.make_contiguous();
                                    let window_start = samples.len() - samples_per_window;
                                    let window = &samples[window_start..];
                                    let newest_sample_timestamp = window
                                        .last()
                                        .map(|sample| {
                                            sample.device_timestamp.unwrap_or(sample.system_timestamp)
                                        })
                                        .unwrap_or(0);
                                    let features =
                                        Self::extract_features(window, buf.estimated_sample_rate_hz);
                                    (features, newest_sample_timestamp)
                                };

                                self.record_signal_latency(newest_sample_timestamp).await;

                                // Broadcast features to hub visualization widgets
                                if let Some(tx) = &self.feature_broadcast_tx {
                                    let _ = tx.send(features.clone());
                                }

                                // Send features to IPC task
                                if self.feature_tx.send(features).await.is_err() {
                                    tracing::warn!("Feature receiver dropped");
                                    break;
                                }
                            }

                            // Update aggregate signal quality across all streams
                            self.update_signal_quality().await;
                        }
                        None => {
                            // Sample sender dropped
                            tracing::info!("Sample channel closed");
                            break;
                        }
                    }
                }

                // Receive ErrP results (for coordinating online learning)
                errp = self.errp_rx.recv() => {
                    if let Some(result) = errp {
                        let mut state = self.state.write().await;
                        let success = (1.0 - result.error_probability).clamp(0.0, 1.0);
                        state.rolling_success_score =
                            state.rolling_success_score * 0.9 + success * 0.1;
                        if result.error_probability > 0.5 {
                            state.errors_detected += 1;
                        }
                    }
                }
            }
        }

        tracing::info!("Signal task processed {} samples", self.sample_count);
        Ok(())
    }

    /// Update the aggregate signal quality across all active stream buffers.
    async fn update_signal_quality(&self) {
        // Compute per-stream quality from the most recent samples, then average
        let mut total_quality = 0.0f32;
        let mut stream_count = 0u32;

        for buf in self.stream_buffers.values() {
            if let Some(last) = buf.samples.back() {
                if let Some(quality) = &last.quality {
                    if !quality.is_empty() {
                        let avg = quality.iter().sum::<f32>() / quality.len() as f32;
                        total_quality += avg;
                        stream_count += 1;
                    }
                }
            }
        }

        if stream_count > 0 {
            let mut state = self.state.write().await;
            state.signal_quality = total_quality / stream_count as f32;
        }
    }

    /// Extracts features from a window of samples.
    ///
    /// This is where the signal processing magic happens. We compute various
    /// features that help the decoder understand what's happening in the brain:
    /// - Band power (how much energy in different frequency ranges)
    /// - Statistical measures (mean, variance)
    /// - Temporal features (changes over time)
    fn extract_features(window: &[Sample], sample_rate_hz: f32) -> FeatureVector {
        let Some(first) = window.first() else {
            return FeatureVector::new(Vec::new());
        };

        let channel_count = first.channel_count();
        if channel_count == 0 {
            return FeatureVector::new(Vec::new());
        }

        let mut channel_data = vec![Vec::with_capacity(window.len()); channel_count];
        let mut timestamps = Vec::with_capacity(window.len());

        for sample in window {
            timestamps.push(sample.device_timestamp.unwrap_or(sample.system_timestamp));
            for (idx, value) in sample.values.iter().enumerate().take(channel_count) {
                channel_data[idx].push(*value);
            }
            if sample.values.len() < channel_count {
                for channel in channel_data
                    .iter_mut()
                    .take(channel_count)
                    .skip(sample.values.len())
                {
                    channel.push(0.0);
                }
            }
        }

        let signal_window = SignalWindow {
            channel_data,
            timestamps,
            channel_count,
            sample_count: window.len(),
        };

        let mut extractor = DspFeatureExtractor::new(DspFeatureConfig {
            sample_rate_hz: if sample_rate_hz.is_finite() {
                sample_rate_hz.clamp(8.0, 2048.0)
            } else {
                128.0
            },
            channel_count,
            welch_segment_len: Self::welch_segment_len_for_window(window.len()),
            ..DspFeatureConfig::default()
        });

        match extractor.extract(&signal_window) {
            Ok(features) => features,
            Err(err) => {
                tracing::warn!("DSP feature extraction failed, using fallback: {}", err);
                Self::extract_features_fallback(window)
            }
        }
    }

    fn extract_features_fallback(window: &[Sample]) -> FeatureVector {
        let num_channels = window.first().map(|s| s.channel_count()).unwrap_or(5);
        let mut features = Vec::with_capacity(num_channels * 4);

        for ch in 0..num_channels {
            let values: Vec<f32> = window.iter().filter_map(|s| s.get(ch)).collect();
            if values.is_empty() {
                features.extend_from_slice(&[0.0, 0.0, 0.0, 0.0]);
                continue;
            }

            let mean = values.iter().sum::<f32>() / values.len() as f32;
            let variance =
                values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32;
            let std_dev = variance.sqrt();
            let power = values.iter().map(|v| v.powi(2)).sum::<f32>() / values.len() as f32;
            let min = values.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let range = max - min;

            features.push(mean);
            features.push(std_dev);
            features.push(power);
            features.push(range);
        }

        for value in &mut features {
            *value = value.clamp(-500.0, 500.0) / 100.0;
        }

        FeatureVector::new(features)
    }

    async fn record_signal_latency(&mut self, sample_timestamp: i64) {
        if sample_timestamp <= 0 {
            return;
        }

        let now_micros = neurohid_types::now_micros();
        let latency_us = now_micros.saturating_sub(sample_timestamp) as u64;
        self.signal_latency.record(latency_us);

        let mut state = self.state.write().await;
        state.signal_latency_last_us = self.signal_latency.last_us();
        state.signal_latency_p95_us = self.signal_latency.p95_us();
    }

    fn maybe_emit_head_movement_marker(&mut self, sample: &Sample) {
        let Some(tx) = &self.marker_broadcast_tx else {
            return;
        };

        let source_id = sample.source_id.as_deref().unwrap_or_default();
        let lower = source_id.to_ascii_lowercase();
        let motion_like =
            lower.contains("motion") || lower.contains("acc") || lower.contains("imu");
        if !motion_like || sample.values.len() < 3 {
            return;
        }

        let x = sample.values[0];
        let y = sample.values[1];
        let z = sample.values[2];
        let magnitude = (x * x + y * y + z * z).sqrt();
        if magnitude < self.head_movement_threshold {
            return;
        }

        let ts = sample.device_timestamp.unwrap_or(sample.system_timestamp);
        if ts.saturating_sub(self.last_head_movement_marker_us) < 250_000 {
            return;
        }
        self.last_head_movement_marker_us = ts;

        let mut marker = StreamMarker::now(MarkerType::HeadMovement)
            .with_payload(MarkerPayload::HeadMovement { magnitude });
        marker.timestamp = ts;
        if let Some(source_id) = &sample.source_id {
            marker = marker.with_source_id(source_id.clone());
        }
        let _ = tx.send(marker);
    }
}

#[cfg(test)]
mod tests {
    use super::SignalTask;

    #[test]
    fn samples_for_duration_uses_expected_rate() {
        assert_eq!(SignalTask::samples_for_duration_ms(500, 128.0), 64);
        assert_eq!(SignalTask::samples_for_duration_ms(50, 128.0), 6);
    }

    #[test]
    fn samples_for_duration_clamps_invalid_rate() {
        assert_eq!(SignalTask::samples_for_duration_ms(500, f32::NAN), 64);
        assert_eq!(SignalTask::samples_for_duration_ms(500, 0.0), 4);
    }

    #[test]
    fn samples_for_duration_never_returns_zero() {
        assert_eq!(SignalTask::samples_for_duration_ms(0, 256.0), 1);
        assert_eq!(SignalTask::samples_for_duration_ms(1, 8.0), 1);
    }

    #[test]
    fn welch_segment_len_is_power_of_two_within_window() {
        assert_eq!(SignalTask::welch_segment_len_for_window(1), 1);
        assert_eq!(SignalTask::welch_segment_len_for_window(2), 2);
        assert_eq!(SignalTask::welch_segment_len_for_window(63), 32);
        assert_eq!(SignalTask::welch_segment_len_for_window(64), 64);
        assert_eq!(SignalTask::welch_segment_len_for_window(200), 128);
        assert_eq!(SignalTask::welch_segment_len_for_window(1024), 256);
    }
}
