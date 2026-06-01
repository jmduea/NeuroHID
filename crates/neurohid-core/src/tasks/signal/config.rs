//! Signal pipeline configuration helpers.
//!
//! Window/step sample computation, Welch segment length, and pipeline
//! construction from `SignalConfig`.

use neurohid_signal::{
    BufferConfig as PipelineBufferConfig, FeatureConfig as PipelineFeatureConfig,
    FilterConfig as PipelineFilterConfig, FilterType, PipelineConfig, SignalPipeline,
};
use neurohid_types::config::SignalConfig;

pub(super) const DEFAULT_STREAM_KEY: &str = "__default__";
pub(super) const SIGNAL_SUMMARY_EVERY_SAMPLES: u64 = 2_048;
pub(super) const SIGNAL_FEATURE_DEBUG_EVERY_SAMPLES: u64 = 512;

/// Samples needed for a given duration in ms at the given sample rate.
pub(super) fn samples_for_duration_ms(duration_ms: u32, sample_rate_hz: f32) -> usize {
    let duration_secs = duration_ms.max(1) as f32 / 1000.0;
    let sample_rate_hz = sanitize_sample_rate_hz(sample_rate_hz);
    (duration_secs * sample_rate_hz).round().max(1.0) as usize
}

/// Welch segment length: largest power-of-two <= sample_count, capped at 256.
pub(super) fn welch_segment_len_for_window(sample_count: usize) -> usize {
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

pub(super) fn sanitize_sample_rate_hz(sample_rate_hz: f32) -> f32 {
    if sample_rate_hz.is_finite() {
        sample_rate_hz.clamp(8.0, 2048.0)
    } else {
        128.0
    }
}

/// Build a signal pipeline from config and stream parameters.
pub(super) fn build_pipeline(
    config: &SignalConfig,
    channel_count: usize,
    nominal_sample_rate_hz: f32,
) -> Option<SignalPipeline> {
    let sample_rate_hz = sanitize_sample_rate_hz(nominal_sample_rate_hz);
    let nyquist = sample_rate_hz / 2.0;

    let mut filters = Vec::new();
    if config.notch_filter_enabled
        && config.notch_filter_hz > 0.0
        && config.notch_filter_hz < nyquist
    {
        filters.push(FilterType::Notch {
            center_hz: config.notch_filter_hz,
            q_factor: 30.0,
        });
    }

    if config.bandpass_filter_enabled {
        let low_hz = config.bandpass_low_hz.max(0.1);
        let high_hz = config.bandpass_high_hz.min(nyquist - 0.1);
        if high_hz > low_hz + 0.05 {
            filters.push(FilterType::Bandpass { low_hz, high_hz });
        }
    }

    let window_samples = samples_for_duration_ms(config.feature_window_ms, sample_rate_hz);
    let step_samples = samples_for_duration_ms(config.feature_step_ms, sample_rate_hz);
    let pipeline_config = PipelineConfig {
        filter: PipelineFilterConfig {
            filters,
            sample_rate_hz,
        },
        buffer: PipelineBufferConfig {
            capacity_samples: config
                .buffer_size_samples
                .max(window_samples)
                .max(step_samples),
            channel_count: channel_count.max(1),
        },
        features: PipelineFeatureConfig {
            sample_rate_hz,
            channel_count: channel_count.max(1),
            welch_segment_len: welch_segment_len_for_window(window_samples),
            ..PipelineFeatureConfig::default()
        },
        artifact_threshold_uv: if config.artifact_rejection_enabled {
            config.artifact_threshold_uv.max(0.0)
        } else {
            f32::MAX
        },
        window_samples,
        step_samples,
        zscore_window_secs: 60.0,
    };

    match SignalPipeline::new(pipeline_config) {
        Ok(pipeline) => Some(pipeline),
        Err(error) => {
            tracing::warn!(
                "Signal pipeline initialization failed; using unfiltered fallback: {}",
                error
            );
            let fallback = PipelineConfig {
                filter: PipelineFilterConfig {
                    filters: Vec::new(),
                    sample_rate_hz,
                },
                buffer: PipelineBufferConfig {
                    capacity_samples: 1024,
                    channel_count: channel_count.max(1),
                },
                features: PipelineFeatureConfig {
                    sample_rate_hz,
                    channel_count: channel_count.max(1),
                    welch_segment_len: 32,
                    ..PipelineFeatureConfig::default()
                },
                artifact_threshold_uv: f32::MAX,
                window_samples: samples_for_duration_ms(500, sample_rate_hz),
                step_samples: samples_for_duration_ms(50, sample_rate_hz),
                zscore_window_secs: 60.0,
            };
            match SignalPipeline::new(fallback) {
                Ok(pipeline) => Some(pipeline),
                Err(fallback_error) => {
                    tracing::error!(
                        "Fallback signal pipeline initialization failed: {}",
                        fallback_error
                    );
                    None
                }
            }
        }
    }
}

/// Human-readable preprocessing summary from config.
pub(super) fn preprocessing_summary(config: &SignalConfig) -> String {
    let notch = if config.notch_filter_enabled {
        format!("notch {:.1}Hz", config.notch_filter_hz)
    } else {
        "notch off".to_string()
    };
    let bandpass = if config.bandpass_filter_enabled {
        format!(
            "bandpass {:.1}-{:.1}Hz",
            config.bandpass_low_hz, config.bandpass_high_hz
        )
    } else {
        "bandpass off".to_string()
    };
    let artifact = if config.artifact_rejection_enabled {
        format!("artifact {:.1}uV", config.artifact_threshold_uv)
    } else {
        "artifact off".to_string()
    };
    format!("{notch}; {bandpass}; {artifact}")
}
