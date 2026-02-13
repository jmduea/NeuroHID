//! # Feature Extraction
//!
//! Transforms a [`SignalWindow`] of filtered EEG data into a 180-dimensional
//! [`FeatureVector`] suitable for the decoder network. Implements Spec §4.4–4.5.
//!
//! ## Feature Composition (180 dimensions for 5-channel Emotiv Insight)
//!
//! | Category            | Dimensions | Method                                     |
//! |---------------------|------------|--------------------------------------------|
//! | Band powers         | 75         | Welch PSD → 5 bands × 5 ch × 3 measures   |
//! | Time-domain stats   | 40         | 5 ch × 8 statistics                        |
//! | Cross-channel       | 15         | 10 correlations + 1 frontal pair × 5 bands |
//! | Temporal            | 50         | 5 bands × 5 ch × 2 trailing measures       |
//!
//! ## BrainFlow Integration Point
//!
//! Band power extraction could be swapped for `DataFilter::get_band_power` and
//! `DataFilter::perform_welch`. The interface (`FeatureExtractor::extract`) and
//! output shape remain the same regardless of backend.

use std::f32::consts::PI;

use neurohid_types::error::SignalError;
use neurohid_types::signal::{FeatureVector, FrequencyBand};
use rustfft::{num_complex::Complex, FftPlanner};

use crate::buffer::SignalWindow;

// ─── Standard EEG frequency bands ────────────────────────────────────────────

/// The five standard bands, ordered for consistent feature vector layout.
const BANDS: [FrequencyBand; 5] = [
    FrequencyBand::Delta,
    FrequencyBand::Theta,
    FrequencyBand::Alpha,
    FrequencyBand::Beta,
    FrequencyBand::Gamma,
];

// ─── Configuration ───────────────────────────────────────────────────────────

/// Configuration for feature extraction.
#[derive(Debug, Clone)]
pub struct FeatureConfig {
    /// Sampling rate of the (filtered) data in the buffer.
    pub sample_rate_hz: f32,

    /// Number of channels.
    pub channel_count: usize,

    /// Welch PSD: FFT segment length in samples. Should be a power of 2.
    /// Default: 64 (0.5s at 128 Hz).
    pub welch_segment_len: usize,

    /// Welch PSD: overlap between segments as a fraction (0.0–1.0).
    /// Default: 0.5 (50% overlap per Spec §4.4).
    pub welch_overlap: f32,

    /// Indices of the two frontal channels used for asymmetry features.
    /// Default: (0, 1) for AF3, AF4 on Emotiv Insight.
    pub frontal_pair: (usize, usize),

    /// Whether to generate feature labels (useful for debugging, slight overhead).
    pub emit_labels: bool,
}

impl Default for FeatureConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 128.0,
            channel_count: 5,
            welch_segment_len: 64,
            welch_overlap: 0.5,
            frontal_pair: (0, 1), // AF3, AF4
            emit_labels: false,
        }
    }
}

// ─── Feature Extractor ───────────────────────────────────────────────────────

/// Extracts the full 180-dimension feature vector from a signal window.
///
/// Stateless per call — all temporal context (trailing averages, slopes) is
/// maintained externally in [`TemporalState`] and passed to `extract_with_temporal`.
pub struct FeatureExtractor {
    config: FeatureConfig,
    planner: FftPlanner<f32>,
}

impl FeatureExtractor {
    pub fn new(config: FeatureConfig) -> Self {
        Self {
            config,
            planner: FftPlanner::new(),
        }
    }

    /// Expected dimensionality of the output feature vector.
    pub fn feature_dim(&self) -> usize {
        let nc = self.config.channel_count;
        let nb = BANDS.len();
        let band_power = nb * nc * 3; // 75
        let time_domain = nc * 8; // 40
        let cross_channel = nc * (nc - 1) / 2   // 10 correlations
            + nb * self.asymmetry_count(); // 25 asymmetries
        let temporal = nb * nc * 2; // 50
        band_power + time_domain + cross_channel + temporal
    }

    /// Number of asymmetry features per band (1 frontal pair → 1 asymmetry).
    fn asymmetry_count(&self) -> usize {
        1
    }

    /// Extract features from a window, without temporal context.
    ///
    /// Temporal features (50 dims) will be zero-filled. Use
    /// [`FeatureExtractor::extract_with_temporal`] for the full 180-dim vector.
    pub fn extract(&mut self, window: &SignalWindow) -> Result<FeatureVector, SignalError> {
        self.extract_with_temporal(window, None)
    }

    /// Extract the complete feature vector, optionally including temporal context.
    pub fn extract_with_temporal(
        &mut self,
        window: &SignalWindow,
        temporal: Option<&TemporalState>,
    ) -> Result<FeatureVector, SignalError> {
        let nc = self.config.channel_count;
        if window.channel_count != nc {
            return Err(SignalError::InvalidChannelConfig(format!(
                "extractor expects {} channels, window has {}",
                nc, window.channel_count
            )));
        }
        if window.sample_count < self.config.welch_segment_len {
            return Err(SignalError::FeatureExtractionFailed(format!(
                "window has {} samples, need at least {} for Welch PSD",
                window.sample_count, self.config.welch_segment_len
            )));
        }

        let mut values = Vec::with_capacity(self.feature_dim());
        let mut labels: Vec<String> = if self.config.emit_labels {
            Vec::with_capacity(self.feature_dim())
        } else {
            Vec::new()
        };

        // ── 1. Band power features (75) ──────────────────────────────────

        // Compute PSD per channel, then integrate per band.
        let mut channel_psds: Vec<Vec<f32>> = Vec::with_capacity(nc);
        let freq_resolution = self.config.sample_rate_hz / self.config.welch_segment_len as f32;

        for ch in 0..nc {
            let data = window.channel(ch).ok_or_else(|| {
                SignalError::FeatureExtractionFailed(format!("missing channel {ch}"))
            })?;
            let psd = self.welch_psd(data)?;
            channel_psds.push(psd);
        }

        // For each band × channel: total power, relative power, peak frequency
        for band in &BANDS {
            let (f_lo, f_hi) = band.range_hz();
            // Clamp gamma upper to Nyquist-safe value
            let f_hi = f_hi.min(self.config.sample_rate_hz / 2.0 - freq_resolution);

            let bin_lo = (f_lo / freq_resolution).ceil() as usize;
            let bin_hi = (f_hi / freq_resolution).floor() as usize;

            for (ch, psd) in channel_psds.iter().enumerate() {
                let total_power: f32 = psd.iter().sum();

                if bin_hi >= psd.len() || bin_lo > bin_hi {
                    // Band falls outside computable range — emit zeros
                    values.extend_from_slice(&[0.0, 0.0, 0.0]);
                    if self.config.emit_labels {
                        labels.push(format!("{band:?}_ch{ch}_total"));
                        labels.push(format!("{band:?}_ch{ch}_relative"));
                        labels.push(format!("{band:?}_ch{ch}_peak_freq"));
                    }
                    continue;
                }

                let band_power: f32 = psd[bin_lo..=bin_hi].iter().sum();
                let relative = if total_power > 0.0 {
                    band_power / total_power
                } else {
                    0.0
                };

                // Peak frequency within band
                let peak_bin = bin_lo
                    + psd[bin_lo..=bin_hi]
                        .iter()
                        .enumerate()
                        .max_by(|(_, a), (_, b)| {
                            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                let peak_freq = peak_bin as f32 * freq_resolution;

                values.push(band_power);
                values.push(relative);
                values.push(peak_freq);

                if self.config.emit_labels {
                    labels.push(format!("{band:?}_ch{ch}_total"));
                    labels.push(format!("{band:?}_ch{ch}_relative"));
                    labels.push(format!("{band:?}_ch{ch}_peak_freq"));
                }
            }
        }

        // ── 2. Time-domain features (40) ─────────────────────────────────

        for ch in 0..nc {
            let data = window.channel(ch).unwrap();
            let stats = time_domain_stats(data);
            values.extend_from_slice(&stats);

            if self.config.emit_labels {
                for name in &[
                    "mean",
                    "std",
                    "skewness",
                    "kurtosis",
                    "hjorth_mobility",
                    "hjorth_complexity",
                    "zero_crossing_rate",
                    "peak_to_peak",
                ] {
                    labels.push(format!("ch{ch}_{name}"));
                }
            }
        }

        // ── 3. Cross-channel features (35) ───────────────────────────────

        // 3a. Pairwise correlations — C(n,2) = 10 for n=5
        for i in 0..nc {
            for j in (i + 1)..nc {
                let corr =
                    pearson_correlation(window.channel(i).unwrap(), window.channel(j).unwrap());
                values.push(corr);
                if self.config.emit_labels {
                    labels.push(format!("corr_ch{i}_ch{j}"));
                }
            }
        }

        // 3b. Frontal asymmetry per band (5 asymmetries)
        let (fl, fr) = self.config.frontal_pair;
        let frontal_pair_in_bounds = fl < channel_psds.len() && fr < channel_psds.len();
        for band in &BANDS {
            let (f_lo, f_hi) = band.range_hz();
            let f_hi = f_hi.min(self.config.sample_rate_hz / 2.0 - freq_resolution);
            let bin_lo = (f_lo / freq_resolution).ceil() as usize;
            let bin_hi = (f_hi / freq_resolution).floor() as usize;

            let asym = if frontal_pair_in_bounds
                && bin_hi < channel_psds[fl].len()
                && bin_hi < channel_psds[fr].len()
                && bin_lo <= bin_hi
            {
                let power_left: f32 = channel_psds[fl][bin_lo..=bin_hi].iter().sum();
                let power_right: f32 = channel_psds[fr][bin_lo..=bin_hi].iter().sum();
                // log asymmetry: log(right) - log(left), per Spec §4.4
                safe_log(power_right) - safe_log(power_left)
            } else {
                0.0
            };
            values.push(asym);
            if self.config.emit_labels {
                labels.push(format!("asym_{band:?}"));
            }
        }

        // ── 4. Temporal features (50) ────────────────────────────────────
        //
        // These require historical context. If no TemporalState is provided,
        // we emit zeros (e.g., during the first few seconds of a session).

        for band in &BANDS {
            let (f_lo, f_hi) = band.range_hz();
            let f_hi = f_hi.min(self.config.sample_rate_hz / 2.0 - freq_resolution);
            let bin_lo = (f_lo / freq_resolution).ceil() as usize;
            let bin_hi = (f_hi / freq_resolution).floor() as usize;

            for (ch, psd) in channel_psds.iter().enumerate() {
                let current_power: f32 = if bin_hi < psd.len() && bin_lo <= bin_hi {
                    psd[bin_lo..=bin_hi].iter().sum()
                } else {
                    0.0
                };

                let (relative_to_avg, slope) = match temporal {
                    Some(state) => state.get(band, ch, current_power),
                    None => (0.0, 0.0),
                };

                values.push(relative_to_avg);
                values.push(slope);

                if self.config.emit_labels {
                    labels.push(format!("{band:?}_ch{ch}_rel_avg"));
                    labels.push(format!("{band:?}_ch{ch}_slope"));
                }
            }
        }

        debug_assert_eq!(
            values.len(),
            self.feature_dim(),
            "feature dimension mismatch: expected {}, got {}",
            self.feature_dim(),
            values.len()
        );

        // Check for NaN/Inf
        for (i, v) in values.iter_mut().enumerate() {
            if !v.is_finite() {
                tracing::warn!(
                    feature_index = i,
                    "non-finite feature value, replacing with 0.0"
                );
                *v = 0.0;
            }
        }

        let mut fv = FeatureVector::new(values);
        if self.config.emit_labels {
            fv.labels = Some(labels);
        }
        Ok(fv)
    }

    /// Welch's PSD estimate for a single channel.
    ///
    /// Splits data into overlapping segments, applies a Hanning window,
    /// computes FFT magnitude², and averages across segments.
    /// Returns one-sided PSD with `segment_len / 2 + 1` bins.
    fn welch_psd(&mut self, data: &[f32]) -> Result<Vec<f32>, SignalError> {
        let seg_len = self.config.welch_segment_len;
        let overlap_samples = (seg_len as f32 * self.config.welch_overlap) as usize;
        let step = seg_len - overlap_samples;

        if data.len() < seg_len {
            return Err(SignalError::FeatureExtractionFailed(
                "data shorter than Welch segment length".into(),
            ));
        }

        // Precompute Hanning window and its power (for normalization)
        let hanning: Vec<f32> = (0..seg_len)
            .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / seg_len as f32).cos()))
            .collect();
        let window_power: f32 = hanning.iter().map(|w| w * w).sum::<f32>() / seg_len as f32;

        let fft = self.planner.plan_fft_forward(seg_len);
        let output_len = seg_len / 2 + 1;
        let mut psd_accum = vec![0.0f32; output_len];
        let mut segment_count = 0u32;

        let mut start = 0;
        while start + seg_len <= data.len() {
            // Apply window and convert to complex
            let mut buf: Vec<Complex<f32>> = data[start..start + seg_len]
                .iter()
                .zip(hanning.iter())
                .map(|(&x, &w)| Complex::new(x * w, 0.0))
                .collect();

            fft.process(&mut buf);

            // Accumulate one-sided magnitude²
            for (i, bin) in buf.iter().enumerate().take(output_len) {
                let mag_sq = bin.norm_sqr();
                // Double non-DC, non-Nyquist bins to account for negative frequencies
                let scale = if i == 0 || i == seg_len / 2 { 1.0 } else { 2.0 };
                psd_accum[i] += mag_sq * scale;
            }
            segment_count += 1;
            start += step;
        }

        if segment_count == 0 {
            return Err(SignalError::FeatureExtractionFailed(
                "no complete Welch segments".into(),
            ));
        }

        // Normalize: average over segments, divide by fs * window_power
        let norm = segment_count as f32 * self.config.sample_rate_hz * window_power;
        for val in &mut psd_accum {
            *val /= norm;
        }

        Ok(psd_accum)
    }
}

// ─── Temporal State ──────────────────────────────────────────────────────────

/// Maintains running averages and recent history for temporal features.
///
/// The pipeline updates this after each feature extraction cycle.
/// Spec §4.4: "Power relative to 10-second trailing average" and
/// "Slope of power over last 5 windows."
pub struct TemporalState {
    /// Running average of band power per (band_index, channel).
    /// Key: `band_idx * channel_count + ch`.
    running_avg: Vec<f32>,

    /// Recent band powers for slope computation (last 5 windows).
    /// Stored as a ring: `history[key][history_pos]`.
    history: Vec<Vec<f32>>,

    /// Number of updates received (for warm-up).
    update_count: u64,

    /// Exponential moving average decay factor.
    /// For a 10-second average at 20 Hz extraction: α ≈ 1/200 = 0.005.
    ema_alpha: f32,

    channel_count: usize,
}

impl TemporalState {
    pub fn new(channel_count: usize, extraction_rate_hz: f32) -> Self {
        let n = BANDS.len() * channel_count;
        // 10-second trailing average
        let ema_alpha = 1.0 / (10.0 * extraction_rate_hz);

        Self {
            running_avg: vec![0.0; n],
            history: vec![Vec::with_capacity(5); n],
            update_count: 0,
            ema_alpha,
            channel_count,
        }
    }

    fn key(&self, band: &FrequencyBand, ch: usize) -> usize {
        let band_idx = BANDS.iter().position(|b| b == band).unwrap_or(0);
        band_idx * self.channel_count + ch
    }

    /// Get temporal features for a given band/channel given the current power.
    ///
    /// Returns `(relative_to_avg, slope)`.
    fn get(&self, band: &FrequencyBand, ch: usize, current_power: f32) -> (f32, f32) {
        let k = self.key(band, ch);

        // Relative to trailing average
        let avg = self.running_avg[k];
        let relative = if avg.abs() > 1e-10 {
            current_power / avg
        } else {
            1.0
        };

        // Slope over recent history via simple linear regression
        let slope = if self.history[k].len() >= 2 {
            simple_slope(&self.history[k])
        } else {
            0.0
        };

        (relative, slope)
    }

    /// Update state with band powers from the latest extraction.
    ///
    /// `band_powers[band_idx][ch]` is the total band power.
    pub fn update(&mut self, band_powers: &[Vec<f32>]) {
        for (band_idx, band_ch_powers) in band_powers.iter().enumerate() {
            for (ch, &power) in band_ch_powers.iter().enumerate() {
                let k = band_idx * self.channel_count + ch;

                // EMA update
                if self.update_count == 0 {
                    self.running_avg[k] = power;
                } else {
                    self.running_avg[k] =
                        self.ema_alpha * power + (1.0 - self.ema_alpha) * self.running_avg[k];
                }

                // Push to history ring (keep last 5)
                if self.history[k].len() >= 5 {
                    self.history[k].remove(0);
                }
                self.history[k].push(power);
            }
        }
        self.update_count += 1;
    }

    /// Number of updates received.
    pub fn update_count(&self) -> u64 {
        self.update_count
    }
}

// ─── Helper functions ────────────────────────────────────────────────────────

/// Compute 8 time-domain statistics for a single channel.
///
/// Returns: [mean, std, skewness, kurtosis, hjorth_mobility, hjorth_complexity,
///           zero_crossing_rate, peak_to_peak]
fn time_domain_stats(data: &[f32]) -> [f32; 8] {
    let n = data.len() as f32;
    if n < 2.0 {
        return [0.0; 8];
    }

    // Mean
    let mean: f32 = data.iter().sum::<f32>() / n;

    // Variance (sample variance with Bessel's correction)
    let var: f32 = data.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / (n - 1.0);
    let std = var.sqrt();

    // Skewness and kurtosis (Fisher's definitions)
    let (skewness, kurtosis) = if std > 1e-10 {
        let m3: f32 = data.iter().map(|x| ((x - mean) / std).powi(3)).sum::<f32>() / n;
        let m4: f32 = data.iter().map(|x| ((x - mean) / std).powi(4)).sum::<f32>() / n;
        (m3, m4 - 3.0) // excess kurtosis
    } else {
        (0.0, 0.0)
    };

    // Hjorth parameters
    // Activity = variance of signal (same as var, but we compute derivatives too)
    // Mobility = sqrt(var(d1) / var(signal))
    // Complexity = mobility(d1) / mobility(signal)
    let d1: Vec<f32> = data.windows(2).map(|w| w[1] - w[0]).collect();
    let d2: Vec<f32> = d1.windows(2).map(|w| w[1] - w[0]).collect();

    let var_d1 = variance(&d1);
    let var_d2 = variance(&d2);

    let mobility = if var > 1e-10 {
        (var_d1 / var).sqrt()
    } else {
        0.0
    };

    let mobility_d1 = if var_d1 > 1e-10 {
        (var_d2 / var_d1).sqrt()
    } else {
        0.0
    };

    let complexity = if mobility > 1e-10 {
        mobility_d1 / mobility
    } else {
        0.0
    };

    // Zero-crossing rate
    let zero_crossings = data
        .windows(2)
        .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
        .count();
    let zcr = zero_crossings as f32 / (n - 1.0);

    // Peak-to-peak amplitude
    let (min_val, max_val) = data
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    let peak_to_peak = max_val - min_val;

    [
        mean,
        std,
        skewness,
        kurtosis,
        mobility,
        complexity,
        zcr,
        peak_to_peak,
    ]
}

/// Sample variance of a slice (Bessel-corrected).
fn variance(data: &[f32]) -> f32 {
    let n = data.len() as f32;
    if n < 2.0 {
        return 0.0;
    }
    let mean: f32 = data.iter().sum::<f32>() / n;
    data.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / (n - 1.0)
}

/// Pearson correlation coefficient between two equal-length slices.
fn pearson_correlation(x: &[f32], y: &[f32]) -> f32 {
    debug_assert_eq!(x.len(), y.len());
    let n = x.len() as f32;
    if n < 2.0 {
        return 0.0;
    }
    let mean_x: f32 = x.iter().sum::<f32>() / n;
    let mean_y: f32 = y.iter().sum::<f32>() / n;

    let mut cov = 0.0f32;
    let mut var_x = 0.0f32;
    let mut var_y = 0.0f32;

    for (&xi, &yi) in x.iter().zip(y.iter()) {
        let dx = xi - mean_x;
        let dy = yi - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }

    let denom = (var_x * var_y).sqrt();
    if denom > 1e-10 {
        cov / denom
    } else {
        0.0
    }
}

/// log-safe: returns ln(x) but clamps x to a small positive value to avoid -inf.
fn safe_log(x: f32) -> f32 {
    (x.max(1e-12)).ln()
}

/// Simple linear regression slope over evenly-spaced values.
/// x-axis is implicitly 0, 1, 2, ... n-1.
fn simple_slope(values: &[f32]) -> f32 {
    let n = values.len() as f32;
    if n < 2.0 {
        return 0.0;
    }
    let x_mean = (n - 1.0) / 2.0;
    let y_mean: f32 = values.iter().sum::<f32>() / n;

    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for (i, &y) in values.iter().enumerate() {
        let dx = i as f32 - x_mean;
        num += dx * (y - y_mean);
        den += dx * dx;
    }

    if den.abs() > 1e-10 {
        num / den
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::SignalWindow;
    use approx::assert_relative_eq;

    /// Build a synthetic 5-channel window with known properties.
    fn make_test_window(num_samples: usize, channels: usize) -> SignalWindow {
        let mut channel_data = Vec::with_capacity(channels);
        for ch in 0..channels {
            let data: Vec<f32> = (0..num_samples)
                .map(|i| {
                    let t = i as f32 / 128.0;
                    // Each channel gets a different frequency: 5, 10, 15, 20, 25 Hz
                    let freq = (ch + 1) as f32 * 5.0;
                    (2.0 * PI * freq * t).sin()
                })
                .collect();
            channel_data.push(data);
        }
        let timestamps: Vec<i64> = (0..num_samples as i64).collect();
        SignalWindow {
            channel_data,
            timestamps,
            channel_count: channels,
            sample_count: num_samples,
        }
    }

    #[test]
    fn test_feature_dimension() {
        let config = FeatureConfig::default();
        let extractor = FeatureExtractor::new(config);
        assert_eq!(extractor.feature_dim(), 180);
    }

    #[test]
    fn test_extract_produces_correct_dim() {
        let config = FeatureConfig::default();
        let mut extractor = FeatureExtractor::new(config);
        let window = make_test_window(128, 5); // 1 second at 128 Hz

        let fv = extractor.extract(&window).unwrap();
        assert_eq!(fv.dim(), 180);
    }

    #[test]
    fn test_extract_no_nans() {
        let config = FeatureConfig::default();
        let mut extractor = FeatureExtractor::new(config);
        let window = make_test_window(128, 5);

        let fv = extractor.extract(&window).unwrap();
        for (i, &v) in fv.values.iter().enumerate() {
            assert!(v.is_finite(), "NaN/Inf at feature index {i}");
        }
    }

    #[test]
    fn test_extract_with_labels() {
        let config = FeatureConfig {
            emit_labels: true,
            ..Default::default()
        };
        let mut extractor = FeatureExtractor::new(config);
        let window = make_test_window(128, 5);

        let fv = extractor.extract(&window).unwrap();
        let labels = fv.labels.as_ref().unwrap();
        assert_eq!(labels.len(), fv.dim());
    }

    #[test]
    fn test_time_domain_stats_dc() {
        // Constant signal: mean=5, std≈0, skewness=0, kurtosis=0
        let data = vec![5.0; 100];
        let stats = time_domain_stats(&data);
        assert_relative_eq!(stats[0], 5.0, epsilon = 0.01); // mean
        assert_relative_eq!(stats[1], 0.0, epsilon = 0.01); // std
        assert_relative_eq!(stats[6], 0.0, epsilon = 0.01); // zcr (no crossings)
        assert_relative_eq!(stats[7], 0.0, epsilon = 0.01); // peak-to-peak
    }

    #[test]
    fn test_time_domain_stats_sine() {
        // Unit sine: mean≈0, std≈0.707, peak-to-peak≈2
        let data: Vec<f32> = (0..1000)
            .map(|i| (2.0 * PI * i as f32 / 1000.0).sin())
            .collect();
        let stats = time_domain_stats(&data);
        assert_relative_eq!(stats[0], 0.0, epsilon = 0.01); // mean
        assert_relative_eq!(stats[1], std::f32::consts::FRAC_1_SQRT_2, epsilon = 0.02); // std ≈ 1/√2
        assert_relative_eq!(stats[7], 2.0, epsilon = 0.01); // peak-to-peak
    }

    #[test]
    fn test_pearson_correlation_identical() {
        let x: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let corr = pearson_correlation(&x, &x);
        assert_relative_eq!(corr, 1.0, epsilon = 0.001);
    }

    #[test]
    fn test_pearson_correlation_inverse() {
        let x: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let y: Vec<f32> = (0..100).map(|i| -(i as f32)).collect();
        let corr = pearson_correlation(&x, &y);
        assert_relative_eq!(corr, -1.0, epsilon = 0.001);
    }

    #[test]
    fn test_simple_slope_linear() {
        // y = 2x → slope = 2
        let values = vec![0.0, 2.0, 4.0, 6.0, 8.0];
        assert_relative_eq!(simple_slope(&values), 2.0, epsilon = 0.001);
    }

    #[test]
    fn test_welch_psd_sine() {
        // A 10 Hz sine at 128 Hz sampling should show a peak near bin 10*64/128 = 5
        let config = FeatureConfig {
            sample_rate_hz: 128.0,
            welch_segment_len: 64,
            ..Default::default()
        };
        let mut extractor = FeatureExtractor::new(config);

        let data: Vec<f32> = (0..256)
            .map(|i| (2.0 * PI * 10.0 * i as f32 / 128.0).sin())
            .collect();

        let psd = extractor.welch_psd(&data).unwrap();
        // Peak should be at bin 5 (10 Hz * 64 / 128)
        let peak_bin = psd
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap()
            .0;
        assert_eq!(peak_bin, 5, "PSD peak should be at bin 5 (10 Hz)");
    }

    #[test]
    fn test_temporal_state_tracks_average() {
        let mut state = TemporalState::new(5, 20.0);

        // Feed constant band powers for a while
        let constant_powers: Vec<Vec<f32>> = vec![vec![1.0; 5]; 5];
        for _ in 0..200 {
            state.update(&constant_powers);
        }

        // Average should converge near 1.0
        let (rel, _slope) = state.get(&FrequencyBand::Alpha, 0, 1.0);
        assert_relative_eq!(rel, 1.0, epsilon = 0.1);
    }

    #[test]
    fn test_window_too_short() {
        let config = FeatureConfig {
            welch_segment_len: 64,
            ..Default::default()
        };
        let mut extractor = FeatureExtractor::new(config);

        let window = make_test_window(32, 5); // too short for 64-sample FFT
        let result = extractor.extract(&window);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_single_channel_with_default_frontal_pair_does_not_panic() {
        let config = FeatureConfig {
            channel_count: 1,
            ..Default::default()
        };
        let mut extractor = FeatureExtractor::new(config);
        let window = make_test_window(128, 1);

        let result = extractor.extract(&window);
        assert!(result.is_ok());
        let fv = result.unwrap();
        assert_eq!(fv.dim(), extractor.feature_dim());
    }
}
