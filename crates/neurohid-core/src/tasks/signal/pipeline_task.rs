//! Per-stream pipeline state and sample processing.
//!
//! `StreamBuffer` holds the pipeline and metrics for one stream; it receives
//! samples, tracks sequence/timestamp integrity, and yields feature vectors.

use neurohid_signal::SignalPipeline;
use neurohid_types::config::SignalConfig;
use neurohid_types::signal::Sample;

use super::config::{build_pipeline, preprocessing_summary, sanitize_sample_rate_hz};

/// Sequence integrity issue for a stream.
#[derive(Debug, Clone, Copy)]
pub(super) enum SignalSequenceIssue {
    Gap {
        previous: u64,
        current: u64,
        missing: u64,
    },
    Regression {
        previous: u64,
        current: u64,
    },
}

/// Per-stream processing state.
pub(super) struct StreamBuffer {
    pub(super) pipeline: Option<SignalPipeline>,
    pub(super) channel_count: usize,
    pub(super) estimated_sample_rate_hz: f32,
    pub(super) last_sample_timestamp_micros: Option<i64>,
    pub(super) last_sequence_number: Option<u64>,
    pub(super) samples_received: u64,
    pub(super) samples_dropped: u64,
    pub(super) integrity_issues: u64,
    pub(super) preprocessing_summary: String,
    pub(super) last_quality: Option<Vec<f32>>,
}

impl StreamBuffer {
    pub(super) fn new(
        config: &SignalConfig,
        channel_count: usize,
        nominal_sample_rate_hz: f32,
    ) -> Self {
        let sample_rate_hz = sanitize_sample_rate_hz(nominal_sample_rate_hz);
        Self {
            pipeline: build_pipeline(config, channel_count, sample_rate_hz),
            channel_count: channel_count.max(1),
            estimated_sample_rate_hz: sample_rate_hz,
            last_sample_timestamp_micros: None,
            last_sequence_number: None,
            samples_received: 0,
            samples_dropped: 0,
            integrity_issues: 0,
            preprocessing_summary: preprocessing_summary(config),
            last_quality: None,
        }
    }

    pub(super) fn rebuild_pipeline(&mut self, config: &SignalConfig) {
        self.pipeline = build_pipeline(
            config,
            self.channel_count,
            self.estimated_sample_rate_hz,
        );
        self.preprocessing_summary = preprocessing_summary(config);
    }

    pub(super) fn update_sample_rate_estimate(&mut self, sample: &Sample) -> Option<(i64, i64)> {
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
            } else {
                self.integrity_issues = self.integrity_issues.saturating_add(1);
                self.last_sample_timestamp_micros = Some(timestamp);
                return Some((previous_timestamp, timestamp));
            }
        }

        self.last_sample_timestamp_micros = Some(timestamp);
        None
    }

    pub(super) fn record_sequence(&mut self, sequence_number: Option<u64>) -> Option<SignalSequenceIssue> {
        let sequence_number = sequence_number?;

        if let Some(previous) = self.last_sequence_number {
            let expected = previous.saturating_add(1);
            if sequence_number > expected {
                let missing = sequence_number.saturating_sub(expected);
                self.samples_dropped = self.samples_dropped.saturating_add(missing);
                self.integrity_issues = self.integrity_issues.saturating_add(1);
                self.last_sequence_number = Some(sequence_number);
                return Some(SignalSequenceIssue::Gap {
                    previous,
                    current: sequence_number,
                    missing,
                });
            } else if sequence_number <= previous {
                self.integrity_issues = self.integrity_issues.saturating_add(1);
                self.last_sequence_number = Some(sequence_number);
                return Some(SignalSequenceIssue::Regression {
                    previous,
                    current: sequence_number,
                });
            }
        }

        self.last_sequence_number = Some(sequence_number);
        None
    }

    pub(super) fn drop_rate_pct(&self) -> Option<f32> {
        let total = self.samples_received.saturating_add(self.samples_dropped);
        if total == 0 {
            None
        } else {
            Some((self.samples_dropped as f32 / total as f32) * 100.0)
        }
    }

    pub(super) fn integrity_state(&self) -> &'static str {
        if self.integrity_issues == 0 {
            "ok"
        } else {
            "degraded"
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct StreamRuntimeMetrics {
    pub(super) effective_sample_rate_hz: Option<f64>,
    pub(super) samples_received: Option<u64>,
    pub(super) samples_dropped: Option<u64>,
    pub(super) drop_rate_pct: Option<f32>,
    pub(super) last_sample_age_ms: Option<u64>,
    pub(super) preprocessing_summary: Option<String>,
    pub(super) integrity_state: Option<String>,
}
