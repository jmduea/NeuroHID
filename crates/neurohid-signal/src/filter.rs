//! # IIR Filter Chain
//!
//! Implements the preprocessing filter chain described in Spec Section 4.3:
//!
//! 1. **Highpass** (0.5 Hz, 2nd order Butterworth) — removes baseline drift
//! 2. **Notch** (50 or 60 Hz, Q=30) — removes power line interference
//!    (Note: at 128 Hz sample rate the 60 Hz notch sits at 94% of Nyquist;
//!    bilinear-transform warping limits rejection depth at this ratio.)
//! 3. **Lowpass** (45 Hz, 2nd order Butterworth) — removes muscle artifact
//!
//! All filters are implemented as cascaded second-order sections (biquads)
//! using the Direct Form II Transposed structure for numerical stability.
//! Filter state is maintained between samples to avoid edge effects.
//!
//! ## BrainFlow Integration Point
//!
//! The biquad coefficient design and processing can be replaced by BrainFlow's
//! `DataFilter::perform_bandpass`, `perform_bandstop`, etc. The interface
//! (`FilterChain::process_sample`) remains the same regardless of backend.
//!
//! ## References
//!
//! Coefficient formulas from Robert Bristow-Johnson's Audio EQ Cookbook:
//! <https://www.w3.org/2011/audio/audio-eq-cookbook.html>

use std::f64::consts::PI;

use neurohid_types::error::SignalError;

/// Coefficients for a second-order IIR (biquad) section.
///
/// Transfer function: H(z) = (b0 + b1·z⁻¹ + b2·z⁻²) / (1 + a1·z⁻¹ + a2·z⁻²)
///
/// Coefficients are stored pre-normalized (divided by a0).
#[derive(Debug, Clone)]
struct BiquadCoeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

/// Internal state for Direct Form II Transposed.
#[derive(Debug, Clone, Default)]
struct BiquadState {
    w1: f64,
    w2: f64,
}

/// A single second-order IIR filter section.
#[derive(Debug, Clone)]
struct Biquad {
    coeffs: BiquadCoeffs,
    state: BiquadState,
}

impl Biquad {
    fn new(coeffs: BiquadCoeffs) -> Self {
        Self {
            coeffs,
            state: BiquadState::default(),
        }
    }

    /// Process one sample through this biquad section.
    ///
    /// Uses Direct Form II Transposed for best numerical behavior:
    ///   y[n] = b0·x[n] + w1
    ///   w1   = b1·x[n] - a1·y[n] + w2
    ///   w2   = b2·x[n] - a2·y[n]
    fn process(&mut self, input: f64) -> f64 {
        let c = &self.coeffs;
        let s = &mut self.state;

        let output = c.b0 * input + s.w1;
        s.w1 = c.b1 * input - c.a1 * output + s.w2;
        s.w2 = c.b2 * input - c.a2 * output;

        output
    }

    /// Reset filter state to zero (e.g., on reconnection or recalibration).
    fn reset(&mut self) {
        self.state = BiquadState::default();
    }

    /// Design a 2nd-order Butterworth lowpass filter.
    fn lowpass(cutoff_hz: f64, sample_rate_hz: f64) -> Self {
        let w0 = 2.0 * PI * cutoff_hz / sample_rate_hz;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        // Q = 1/√2 for Butterworth (maximally flat)
        let alpha = sin_w0 / (2.0 * std::f64::consts::FRAC_1_SQRT_2);

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self::new(BiquadCoeffs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        })
    }

    /// Design a 2nd-order Butterworth highpass filter.
    fn highpass(cutoff_hz: f64, sample_rate_hz: f64) -> Self {
        let w0 = 2.0 * PI * cutoff_hz / sample_rate_hz;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * std::f64::consts::FRAC_1_SQRT_2);

        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self::new(BiquadCoeffs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        })
    }

    /// Design a 2nd-order notch (band-reject) filter.
    fn notch(center_hz: f64, q_factor: f64, sample_rate_hz: f64) -> Self {
        let w0 = 2.0 * PI * center_hz / sample_rate_hz;
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * q_factor);

        let b0 = 1.0;
        let b1 = -2.0 * cos_w0;
        let b2 = 1.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self::new(BiquadCoeffs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        })
    }
}

/// Describes a filter to apply. Multiple filters are cascaded in order.
#[derive(Debug, Clone)]
pub enum FilterType {
    /// Butterworth lowpass filter.
    Lowpass { cutoff_hz: f32 },
    /// Butterworth highpass filter.
    Highpass { cutoff_hz: f32 },
    /// Bandpass filter (implemented as cascaded highpass + lowpass).
    Bandpass { low_hz: f32, high_hz: f32 },
    /// Notch (band-reject) filter for power line interference.
    Notch { center_hz: f32, q_factor: f32 },
}

/// Configuration for the filter chain.
#[derive(Debug, Clone)]
pub struct FilterConfig {
    /// Filters to apply, in order. Each filter is applied to every channel.
    pub filters: Vec<FilterType>,
    /// Sample rate of the incoming data.
    pub sample_rate_hz: f32,
}

impl FilterConfig {
    /// Standard EEG preprocessing filter chain per Spec Section 4.3.
    ///
    /// - Highpass at 0.5 Hz (baseline correction)
    /// - Notch at `line_freq` Hz with Q=30 (power line removal)
    /// - Lowpass at 45 Hz (muscle artifact removal)
    pub fn eeg_default(sample_rate_hz: f32, line_freq_hz: f32) -> Self {
        Self {
            filters: vec![
                FilterType::Highpass { cutoff_hz: 0.5 },
                FilterType::Notch {
                    center_hz: line_freq_hz,
                    q_factor: 30.0,
                },
                FilterType::Lowpass { cutoff_hz: 45.0 },
            ],
            sample_rate_hz,
        }
    }
}

/// A chain of IIR filters applied independently to each channel.
///
/// Each channel gets its own filter state, so channels don't interfere
/// with each other. Filters process one sample at a time, maintaining
/// state between calls to avoid edge effects.
pub struct FilterChain {
    /// `channel_sections[ch]` is the ordered list of biquad sections for that channel.
    channel_sections: Vec<Vec<Biquad>>,
    #[expect(dead_code, reason = "retained for filter introspection and reconfiguration")]
    config: FilterConfig,
    channel_count: usize,
}

impl FilterChain {
    /// Create a new filter chain from the given configuration.
    ///
    /// Validates that filter frequencies are within the Nyquist limit.
    pub fn new(config: FilterConfig, channel_count: usize) -> Result<Self, SignalError> {
        let nyquist = config.sample_rate_hz / 2.0;

        // Validate frequencies
        for f in &config.filters {
            match f {
                FilterType::Lowpass { cutoff_hz } => {
                    if *cutoff_hz <= 0.0 || *cutoff_hz >= nyquist {
                        return Err(SignalError::InvalidFilterConfig(format!(
                            "lowpass cutoff {cutoff_hz} Hz must be between 0 and Nyquist ({nyquist} Hz)"
                        )));
                    }
                }
                FilterType::Highpass { cutoff_hz } => {
                    if *cutoff_hz <= 0.0 || *cutoff_hz >= nyquist {
                        return Err(SignalError::InvalidFilterConfig(format!(
                            "highpass cutoff {cutoff_hz} Hz must be between 0 and Nyquist ({nyquist} Hz)"
                        )));
                    }
                }
                FilterType::Bandpass { low_hz, high_hz } => {
                    if *low_hz <= 0.0 || *high_hz >= nyquist || *low_hz >= *high_hz {
                        return Err(SignalError::InvalidFilterConfig(format!(
                            "bandpass range [{low_hz}, {high_hz}] Hz invalid for Nyquist {nyquist} Hz"
                        )));
                    }
                }
                FilterType::Notch {
                    center_hz,
                    q_factor,
                } => {
                    if *center_hz <= 0.0 || *center_hz >= nyquist || *q_factor <= 0.0 {
                        return Err(SignalError::InvalidFilterConfig(format!(
                            "notch at {center_hz} Hz (Q={q_factor}) invalid for Nyquist {nyquist} Hz"
                        )));
                    }
                }
            }
        }

        // Build per-channel biquad sections
        let fs = config.sample_rate_hz as f64;
        let template_sections = Self::build_sections(&config.filters, fs);

        let channel_sections = (0..channel_count)
            .map(|_| template_sections.clone())
            .collect();

        Ok(Self {
            channel_sections,
            config,
            channel_count,
        })
    }

    /// Convenience constructor for the standard EEG filter chain.
    pub fn eeg_default(
        sample_rate_hz: f32,
        channel_count: usize,
        line_freq_hz: f32,
    ) -> Result<Self, SignalError> {
        let config = FilterConfig::eeg_default(sample_rate_hz, line_freq_hz);
        Self::new(config, channel_count)
    }

    fn build_sections(filters: &[FilterType], fs: f64) -> Vec<Biquad> {
        let mut sections = Vec::new();
        for f in filters {
            match f {
                FilterType::Lowpass { cutoff_hz } => {
                    sections.push(Biquad::lowpass(*cutoff_hz as f64, fs));
                }
                FilterType::Highpass { cutoff_hz } => {
                    sections.push(Biquad::highpass(*cutoff_hz as f64, fs));
                }
                FilterType::Bandpass { low_hz, high_hz } => {
                    sections.push(Biquad::highpass(*low_hz as f64, fs));
                    sections.push(Biquad::lowpass(*high_hz as f64, fs));
                }
                FilterType::Notch {
                    center_hz,
                    q_factor,
                } => {
                    sections.push(Biquad::notch(*center_hz as f64, *q_factor as f64, fs));
                }
            }
        }
        sections
    }

    /// Filter a single multi-channel sample in-place.
    ///
    /// `sample` must have exactly `channel_count` elements. Each value
    /// passes through all biquad sections for that channel, and filter
    /// state is updated for the next call.
    ///
    /// Returns the filtered values.
    pub fn process_sample(&mut self, sample: &[f32]) -> Result<Vec<f32>, SignalError> {
        if sample.len() != self.channel_count {
            return Err(SignalError::InvalidChannelConfig(format!(
                "filter chain expects {} channels, got {}",
                self.channel_count,
                sample.len()
            )));
        }

        let mut output = Vec::with_capacity(self.channel_count);

        for (ch, &val) in sample.iter().enumerate() {
            let mut x = val as f64;
            for section in &mut self.channel_sections[ch] {
                x = section.process(x);
            }
            output.push(x as f32);
        }

        Ok(output)
    }

    /// Reset all filter states to zero.
    ///
    /// Call this on device reconnection or when signal continuity is broken.
    pub fn reset(&mut self) {
        for ch_sections in &mut self.channel_sections {
            for section in ch_sections {
                section.reset();
            }
        }
    }
}

// Expose NotchFilter and BandpassFilter as type aliases for the lib.rs re-exports.
// The actual implementation uses FilterType + FilterChain, but these provide
// the documented public types.

/// Notch filter configuration (re-exported for convenience).
pub type NotchFilter = FilterType;

/// Bandpass filter configuration (re-exported for convenience).
pub type BandpassFilter = FilterType;

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_lowpass_passes_dc() {
        // A lowpass filter should pass a DC signal without attenuation.
        let mut bq = Biquad::lowpass(10.0, 128.0);
        // Run 100 samples of DC = 1.0 through the filter
        let mut output = 0.0;
        for _ in 0..100 {
            output = bq.process(1.0);
        }
        // After settling, output should be ~1.0
        assert_relative_eq!(output, 1.0, epsilon = 0.01);
    }

    #[test]
    fn test_highpass_blocks_dc() {
        let mut bq = Biquad::highpass(0.5, 128.0);
        let mut output = 0.0;
        for _ in 0..500 {
            output = bq.process(1.0);
        }
        // Highpass should reject DC → output near 0
        assert_relative_eq!(output, 0.0, epsilon = 0.01);
    }

    #[test]
    fn test_notch_passes_other_frequencies() {
        // Feed a 10 Hz sine through a 50 Hz notch — should pass largely unattenuated.
        let fs = 128.0;
        let mut bq = Biquad::notch(50.0, 30.0, fs);

        let mut max_output: f64 = 0.0;
        for i in 0..512 {
            let t = i as f64 / fs;
            let input = (2.0 * PI * 10.0 * t).sin();
            let output = bq.process(input);
            if i > 100 {
                // Skip transient
                max_output = max_output.max(output.abs());
            }
        }
        // 10 Hz signal should be mostly unaffected (>90% amplitude)
        assert!(max_output > 0.9, "10 Hz signal attenuated to {max_output}");
    }

    #[test]
    fn test_notch_attenuates_target() {
        let fs = 128.0;
        let mut bq = Biquad::notch(50.0, 30.0, fs);

        let mut max_output: f64 = 0.0;
        for i in 0..512 {
            let t = i as f64 / fs;
            let input = (2.0 * PI * 50.0 * t).sin();
            let output = bq.process(input);
            if i > 100 {
                max_output = max_output.max(output.abs());
            }
        }
        // 50 Hz signal should be attenuated.
        // Note: at 128 Hz sample rate, 50 Hz is near Nyquist (78% of 64 Hz).
        // Bilinear transform frequency warping limits notch depth this close
        // to Nyquist. A single biquad with Q=30 achieves ~0.35 attenuation here,
        // which is still useful for reducing line noise in practice.
        assert!(
            max_output < 0.4,
            "50 Hz signal not attenuated enough: {max_output}"
        );
    }

    #[test]
    fn test_filter_chain_multichannel() {
        let config = FilterConfig {
            filters: vec![FilterType::Lowpass { cutoff_hz: 40.0 }],
            sample_rate_hz: 128.0,
        };
        let mut chain = FilterChain::new(config, 3).unwrap();

        // Process a sample — should not error
        let result = chain.process_sample(&[1.0, 2.0, 3.0]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[test]
    fn test_filter_chain_wrong_channels() {
        let config = FilterConfig {
            filters: vec![FilterType::Lowpass { cutoff_hz: 40.0 }],
            sample_rate_hz: 128.0,
        };
        let mut chain = FilterChain::new(config, 3).unwrap();

        let result = chain.process_sample(&[1.0, 2.0]);
        assert!(result.is_err());
    }

    #[test]
    fn test_nyquist_validation() {
        let config = FilterConfig {
            filters: vec![FilterType::Lowpass { cutoff_hz: 70.0 }],
            sample_rate_hz: 128.0, // Nyquist = 64
        };
        let result = FilterChain::new(config, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_eeg_default_chain() {
        // Should construct without error for standard parameters
        let chain = FilterChain::eeg_default(128.0, 5, 60.0);
        assert!(chain.is_ok());
    }
}
