//! # Signal Processing Pipeline
//!
//! Orchestrates the full signal processing chain from raw EEG samples to
//! normalized feature vectors. This is the primary entry point for the
//! signal processing crate.
//!
//! ## Data Flow
//!
//! ```text
//! Raw Sample
//!     │
//!     ▼
//! Artifact Check ──(rejected)──▶ skip, increment counter
//!     │ (passed)
//!     ▼
//! Filter Chain (stateful IIR, per-sample)
//!     │
//!     ▼
//! Ring Buffer (stores filtered data)
//!     │
//!     ▼ (timer-driven, every step_ms)
//! Extract Window → Feature Extraction → Z-Score Normalize
//!     │
//!     ▼
//! FeatureVector (180 dims, sent to decoder via IPC)
//! ```
//!
//! The pipeline is designed so that `push_sample` is called at the device
//! sample rate (~128 Hz) and `try_extract` is called at the feature rate
//! (~20 Hz). These can happen on the same thread in a single event loop.

use neurohid_types::error::SignalError;
use neurohid_types::signal::FeatureVector;

use crate::buffer::{BufferConfig, SampleBuffer, SignalWindow};
use crate::features::{FeatureConfig, FeatureExtractor, TemporalState};
use crate::filter::{FilterChain, FilterConfig};

// ─── Configuration ───────────────────────────────────────────────────────────

/// Top-level pipeline configuration.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Filter chain configuration.
    pub filter: FilterConfig,

    /// Ring buffer configuration.
    pub buffer: BufferConfig,

    /// Feature extraction configuration.
    pub features: FeatureConfig,

    /// Artifact rejection threshold in microvolts.
    /// Samples where any channel exceeds ±threshold are rejected.
    /// Spec §4.3: "typically ±100 µV".
    pub artifact_threshold_uv: f32,

    /// Feature extraction window length in samples.
    /// Spec §4.4: "500ms length" → 64 samples at 128 Hz.
    pub window_samples: usize,

    /// Feature extraction step in samples.
    /// Spec §4.4: "50ms step" → ~6-7 samples at 128 Hz.
    pub step_samples: usize,

    /// Z-score normalization window in seconds.
    /// Spec §4.5: "60-second window".
    pub zscore_window_secs: f32,
}

impl PipelineConfig {
    /// Default configuration for Emotiv Insight at 128 Hz with 60 Hz line noise.
    pub fn emotiv_insight(line_freq_hz: f32) -> Self {
        let sample_rate = 128.0;
        let channels = 5;
        Self {
            filter: FilterConfig::eeg_default(sample_rate, line_freq_hz),
            buffer: BufferConfig {
                capacity_samples: 1280, // 10 seconds at 128 Hz
                channel_count: channels,
            },
            features: FeatureConfig {
                sample_rate_hz: sample_rate,
                channel_count: channels,
                ..Default::default()
            },
            artifact_threshold_uv: 100.0,
            window_samples: 64,  // 500ms at 128 Hz
            step_samples: 7,     // ~50ms at 128 Hz (128 * 0.05 ≈ 6.4 → 7)
            zscore_window_secs: 60.0,
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self::emotiv_insight(60.0) // North America default
    }
}

// ─── Running Z-Score Normalizer ──────────────────────────────────────────────

/// Maintains running mean and variance for z-score normalization.
///
/// Uses Welford's online algorithm for numerically stable incremental
/// statistics, with an exponential decay to weight recent data.
struct ZScoreNormalizer {
    running_mean: Vec<f32>,
    running_var: Vec<f32>,
    update_count: u64,
    /// EMA decay factor. For a 60-second window at 20 Hz: α ≈ 1/1200.
    alpha: f32,
    dim: usize,
}

impl ZScoreNormalizer {
    fn new(dim: usize, window_secs: f32, extraction_rate_hz: f32) -> Self {
        let alpha = 1.0 / (window_secs * extraction_rate_hz);
        Self {
            running_mean: vec![0.0; dim],
            running_var: vec![1.0; dim], // Start with unit variance to avoid div-by-zero
            update_count: 0,
            alpha,
            dim,
        }
    }

    /// Update running statistics and return z-scored values.
    fn normalize(&mut self, values: &[f32]) -> Vec<f32> {
        debug_assert_eq!(values.len(), self.dim);

        let mut output = Vec::with_capacity(self.dim);

        for (i, &v) in values.iter().enumerate() {
            if self.update_count == 0 {
                // First observation — initialize, output zero
                self.running_mean[i] = v;
                self.running_var[i] = 1.0; // can't estimate variance yet
                output.push(0.0);
            } else {
                // EMA update of mean and variance
                let old_mean = self.running_mean[i];
                let new_mean = self.alpha * v + (1.0 - self.alpha) * old_mean;
                let diff_sq = (v - new_mean).powi(2);
                let new_var =
                    self.alpha * diff_sq + (1.0 - self.alpha) * self.running_var[i];

                self.running_mean[i] = new_mean;
                self.running_var[i] = new_var.max(1e-8); // floor to prevent div-by-zero

                // Z-score: (x - mean) / std
                let std = self.running_var[i].sqrt();
                let z = (v - self.running_mean[i]) / std;
                output.push(z.clamp(-10.0, 10.0)); // clamp extreme outliers
            }
        }

        self.update_count += 1;
        output
    }

    /// Reset running statistics (e.g., after recalibration).
    fn reset(&mut self) {
        self.running_mean.fill(0.0);
        self.running_var.fill(1.0);
        self.update_count = 0;
    }
}

// ─── Pipeline Statistics ─────────────────────────────────────────────────────

/// Counters for monitoring pipeline health.
#[derive(Debug, Clone, Default)]
pub struct PipelineStats {
    /// Total samples received via `push_sample`.
    pub samples_received: u64,
    /// Samples rejected by artifact detection.
    pub samples_rejected: u64,
    /// Feature vectors successfully produced.
    pub features_produced: u64,
}

impl PipelineStats {
    /// Artifact rejection rate as a fraction (0.0 – 1.0).
    pub fn rejection_rate(&self) -> f32 {
        if self.samples_received == 0 {
            0.0
        } else {
            self.samples_rejected as f32 / self.samples_received as f32
        }
    }
}

// ─── Signal Pipeline ─────────────────────────────────────────────────────────

/// The complete signal processing pipeline.
///
/// Usage pattern:
/// ```ignore
/// let mut pipeline = SignalPipeline::new(PipelineConfig::default())?;
///
/// // Called at ~128 Hz as samples arrive from device
/// pipeline.push_sample(&sample.values, sample.system_timestamp)?;
///
/// // Called at ~20 Hz to check if a new feature vector is ready
/// if let Some(features) = pipeline.try_extract()? {
///     // Send features to decoder via IPC
/// }
/// ```
pub struct SignalPipeline {
    filter: FilterChain,
    buffer: SampleBuffer,
    extractor: FeatureExtractor,
    normalizer: ZScoreNormalizer,
    temporal: TemporalState,
    config: PipelineConfig,
    stats: PipelineStats,

    /// Samples pushed since last extraction. Compared against `step_samples`
    /// to decide when a new feature vector is due.
    samples_since_extract: usize,
}

impl SignalPipeline {
    /// Create a new pipeline from configuration.
    pub fn new(config: PipelineConfig) -> Result<Self, SignalError> {
        let filter = FilterChain::new(
            config.filter.clone(),
            config.buffer.channel_count,
        )?;
        let buffer = SampleBuffer::new(config.buffer.clone());
        let extractor = FeatureExtractor::new(config.features.clone());

        let feature_dim = extractor.feature_dim();
        let extraction_rate =
            config.features.sample_rate_hz / config.step_samples as f32;

        let normalizer = ZScoreNormalizer::new(
            feature_dim,
            config.zscore_window_secs,
            extraction_rate,
        );
        let temporal = TemporalState::new(
            config.features.channel_count,
            extraction_rate,
        );

        Ok(Self {
            filter,
            buffer,
            extractor,
            normalizer,
            temporal,
            config,
            stats: PipelineStats::default(),
            samples_since_extract: 0,
        })
    }

    /// Push a raw (unfiltered) multi-channel sample into the pipeline.
    ///
    /// The sample passes through artifact rejection and filtering before
    /// being stored in the ring buffer.
    pub fn push_sample(
        &mut self,
        values: &[f32],
        timestamp: i64,
    ) -> Result<(), SignalError> {
        self.stats.samples_received += 1;

        // ── Artifact rejection ───────────────────────────────────────
        if self.is_artifact(values) {
            self.stats.samples_rejected += 1;
            return Ok(()); // silently discard
        }

        // ── Filter ───────────────────────────────────────────────────
        let filtered = self.filter.process_sample(values)?;

        // ── Buffer ───────────────────────────────────────────────────
        self.buffer.push(&filtered, timestamp)?;
        self.samples_since_extract += 1;

        Ok(())
    }

    /// Try to extract a feature vector if enough new samples have arrived
    /// since the last extraction.
    ///
    /// Returns `Ok(Some(fv))` when a new feature vector is ready,
    /// `Ok(None)` when not enough samples have accumulated yet, or
    /// `Err` if extraction fails.
    pub fn try_extract(&mut self) -> Result<Option<FeatureVector>, SignalError> {
        // Not time for extraction yet
        if self.samples_since_extract < self.config.step_samples {
            return Ok(None);
        }

        // Not enough data in buffer for a full window
        let window = match self.buffer.window(self.config.window_samples) {
            Some(w) => w,
            None => return Ok(None),
        };

        self.samples_since_extract = 0;

        // ── Extract raw features ─────────────────────────────────────
        let raw_fv = self.extractor.extract_with_temporal(&window, Some(&self.temporal))?;

        // ── Update temporal state with current band powers ───────────
        self.update_temporal(&window)?;

        // ── Z-score normalize ────────────────────────────────────────
        let normalized_values = self.normalizer.normalize(&raw_fv.values);
        let mut fv = FeatureVector::new(normalized_values);
        fv.timestamp = raw_fv.timestamp;
        fv.labels = raw_fv.labels;

        self.stats.features_produced += 1;

        Ok(Some(fv))
    }

    /// Check whether a raw sample should be rejected as artifact.
    ///
    /// Spec §4.3: "flagging samples where any channel exceeds a threshold
    /// (typically ±100 µV)".
    fn is_artifact(&self, values: &[f32]) -> bool {
        let threshold = self.config.artifact_threshold_uv;
        values.iter().any(|&v| v.abs() > threshold)
    }

    /// Update the temporal state by extracting current band powers from the window.
    fn update_temporal(&mut self, window: &SignalWindow) -> Result<(), SignalError> {
        // We need band powers per (band, channel). We can compute them
        // from the PSD that the extractor already computes, but to avoid
        // duplicating the FFT we extract a simplified version here.
        let nc = self.config.features.channel_count;
        let bands = [
            neurohid_types::signal::FrequencyBand::Delta,
            neurohid_types::signal::FrequencyBand::Theta,
            neurohid_types::signal::FrequencyBand::Alpha,
            neurohid_types::signal::FrequencyBand::Beta,
            neurohid_types::signal::FrequencyBand::Gamma,
        ];

        let freq_res = self.config.features.sample_rate_hz
            / self.config.features.welch_segment_len as f32;

        // Quick band power estimation using simple DFT energy in band.
        // This is a lightweight approximation — the full Welch PSD was
        // already computed in extract_with_temporal, but we don't cache it
        // to keep the extractor stateless. The temporal update is low-frequency
        // enough (~20 Hz) that this small redundancy is acceptable.
        let mut band_powers: Vec<Vec<f32>> = Vec::with_capacity(bands.len());

        for band in &bands {
            let (f_lo, f_hi) = band.range_hz();
            let f_hi = f_hi.min(self.config.features.sample_rate_hz / 2.0 - freq_res);
            let mut ch_powers = Vec::with_capacity(nc);

            for ch in 0..nc {
                if let Some(data) = window.channel(ch) {
                    // Simple band power: sum of squared values in bandpass range.
                    // This is a rough approximation; acceptable for temporal tracking.
                    let power = band_power_approx(data, f_lo, f_hi, self.config.features.sample_rate_hz);
                    ch_powers.push(power);
                } else {
                    ch_powers.push(0.0);
                }
            }
            band_powers.push(ch_powers);
        }

        self.temporal.update(&band_powers);
        Ok(())
    }

    /// Get the most recent signal window (e.g., for ErrP detection).
    ///
    /// This is used by the core service to capture post-action windows
    /// for Error-Related Potential classification (Spec §6.3).
    pub fn capture_window(&self, num_samples: usize) -> Option<SignalWindow> {
        self.buffer.window(num_samples)
    }

    /// Current pipeline statistics.
    pub fn stats(&self) -> &PipelineStats {
        &self.stats
    }

    /// Reset the pipeline state (filters, buffer, normalizer).
    ///
    /// Call this on device reconnection or recalibration.
    pub fn reset(&mut self) {
        self.filter.reset();
        self.buffer.clear();
        self.normalizer.reset();
        self.samples_since_extract = 0;
        self.stats = PipelineStats::default();
    }

    /// Number of samples currently in the buffer.
    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }

    /// Expected feature vector dimensionality.
    pub fn feature_dim(&self) -> usize {
        self.extractor.feature_dim()
    }
}

/// Quick band power approximation using Goertzel-like energy sum.
///
/// For temporal tracking we don't need Welch-quality PSD — a rough
/// estimate is sufficient and avoids a second FFT per extraction cycle.
fn band_power_approx(data: &[f32], f_lo: f32, f_hi: f32, fs: f32) -> f32 {
    let n = data.len() as f32;
    if n < 4.0 {
        return 0.0;
    }

    // Sum energy at a few probe frequencies across the band
    let num_probes = 4;
    let step = (f_hi - f_lo) / num_probes as f32;
    let mut total = 0.0f32;

    for k in 0..num_probes {
        let freq = f_lo + step * (k as f32 + 0.5);
        let w = 2.0 * std::f32::consts::PI * freq / fs;

        // Goertzel: compute |X(f)|² without full FFT
        let mut s1: f32 = 0.0;
        let mut s2: f32 = 0.0;
        let coeff = 2.0 * w.cos();

        for &x in data {
            let temp = x + coeff * s1 - s2;
            s2 = s1;
            s1 = temp;
        }

        let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
        total += power / (n * n); // normalize
    }

    total / num_probes as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pipeline() -> SignalPipeline {
        SignalPipeline::new(PipelineConfig::default()).unwrap()
    }

    #[test]
    fn test_pipeline_creation() {
        let pipeline = make_pipeline();
        assert_eq!(pipeline.feature_dim(), 180);
        assert_eq!(pipeline.buffer_len(), 0);
    }

    #[test]
    fn test_push_and_extract() {
        let mut pipeline = make_pipeline();

        // Push enough samples for one window + step
        // Window = 64 samples, step = 7 samples
        // Need at least 64 samples in buffer and 7 since last extract
        for i in 0..128 {
            let t = i as f32 / 128.0;
            let values = vec![
                (2.0 * std::f32::consts::PI * 10.0 * t).sin() * 10.0,
                (2.0 * std::f32::consts::PI * 12.0 * t).sin() * 10.0,
                (2.0 * std::f32::consts::PI * 8.0 * t).sin() * 10.0,
                (2.0 * std::f32::consts::PI * 15.0 * t).sin() * 10.0,
                (2.0 * std::f32::consts::PI * 20.0 * t).sin() * 10.0,
            ];
            pipeline
                .push_sample(&values, (i * 7812) as i64) // ~128 Hz timestamps
                .unwrap();
        }

        // Should produce at least one feature vector
        let mut produced = 0;
        for _ in 0..20 {
            if let Some(fv) = pipeline.try_extract().unwrap() {
                assert_eq!(fv.dim(), 180);
                produced += 1;
            }
        }
        // We pushed 128 samples with step=7 and window=64.
        // After 64 samples buffered, extractions happen every 7 samples.
        // (128 - 64) / 7 ≈ 9 possible extractions
        assert!(produced >= 1, "should have produced at least 1 feature vector");
    }

    #[test]
    fn test_artifact_rejection() {
        let mut pipeline = make_pipeline();

        // Push a clean sample
        pipeline.push_sample(&[1.0; 5], 0).unwrap();
        assert_eq!(pipeline.stats().samples_rejected, 0);

        // Push a sample with artifact (>100 µV)
        pipeline.push_sample(&[150.0, 1.0, 1.0, 1.0, 1.0], 1000).unwrap();
        assert_eq!(pipeline.stats().samples_rejected, 1);
        assert_eq!(pipeline.buffer_len(), 1); // artifact wasn't buffered
    }

    #[test]
    fn test_no_extract_before_enough_data() {
        let mut pipeline = make_pipeline();

        // Push just a few samples — not enough for a window
        for i in 0..10 {
            pipeline.push_sample(&[1.0; 5], i * 7812).unwrap();
        }

        assert!(pipeline.try_extract().unwrap().is_none());
    }

    #[test]
    fn test_reset() {
        let mut pipeline = make_pipeline();
        for i in 0..64 {
            pipeline.push_sample(&[1.0; 5], i * 7812).unwrap();
        }
        assert!(pipeline.buffer_len() > 0);

        pipeline.reset();
        assert_eq!(pipeline.buffer_len(), 0);
        assert_eq!(pipeline.stats().samples_received, 0);
    }

    #[test]
    fn test_capture_window_for_errp() {
        let mut pipeline = make_pipeline();

        // Push 1 second of data
        for i in 0..128 {
            pipeline.push_sample(&[5.0; 5], i * 7812).unwrap();
        }

        // Capture a 450ms window for ErrP (≈58 samples at 128 Hz)
        let window = pipeline.capture_window(58);
        assert!(window.is_some());
        assert_eq!(window.unwrap().sample_count, 58);
    }

    #[test]
    fn test_stats_tracking() {
        let mut pipeline = make_pipeline();

        for i in 0..10 {
            pipeline.push_sample(&[1.0; 5], i).unwrap();
        }
        pipeline.push_sample(&[200.0, 1.0, 1.0, 1.0, 1.0], 10).unwrap();

        assert_eq!(pipeline.stats().samples_received, 11);
        assert_eq!(pipeline.stats().samples_rejected, 1);
    }

    #[test]
    fn test_zscore_normalizer_centers_data() {
        let mut norm = ZScoreNormalizer::new(3, 10.0, 20.0);

        // Feed constant values for a while
        for _ in 0..200 {
            norm.normalize(&[5.0, 10.0, 15.0]);
        }

        // After convergence, z-score of the same constant should be ~0
        let result = norm.normalize(&[5.0, 10.0, 15.0]);
        for &v in &result {
            assert!(
                v.abs() < 0.5,
                "z-score of constant should be near 0, got {v}"
            );
        }
    }
}
