//! Broker-side types and helpers for the IPC task.
//!
//! Connection lifecycle, ErrP window buffering, and sample buffers used when
//! bridging the runtime to the trainer (ML bridge).

use std::collections::VecDeque;

use neurohid_types::error::{Error, IpcError};
use neurohid_types::signal::Sample;

/// Constants for broker/task timing and buffering.
pub(super) const REAL_MESSAGE_POLL_MS: u64 = 25;
pub(super) const SIMULATED_CONNECT_DELAY_MS: u64 = 100;
pub(super) const DEFAULT_ERRP_STREAM_KEY: &str = "__all__";
pub(super) const ERRP_BUFFER_RETENTION_US: i64 = 5_000_000;
pub(super) const ERRP_EMIT_GRACE_US: i64 = 120_000;
pub(super) const DEFAULT_ERRP_SAMPLE_RATE_HZ: f32 = 128.0;
pub(super) const MAX_CANDIDATE_FUTURE_SKEW_US: i64 = 5 * 60 * 1_000_000;
pub(super) const MAX_CANDIDATE_MODEL_BYTES: u64 = 128 * 1024 * 1024;
pub(super) const IPC_TELEMETRY_SUMMARY_EVERY: u64 = 120;

#[derive(Debug, Clone)]
pub(super) struct PendingErrpWindow {
    pub(super) decision_id: String,
    pub(super) action_timestamp_us: i64,
    pub(super) window_start_us: i64,
    pub(super) window_end_us: i64,
    pub(super) stream_id: Option<String>,
    pub(super) signal_quality: f32,
}

#[derive(Debug, Clone)]
pub(super) struct StreamSampleBuffer {
    pub(super) samples: VecDeque<Sample>,
}

impl StreamSampleBuffer {
    pub(super) fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(1024),
        }
    }

    pub(super) fn push(&mut self, sample: Sample) {
        self.samples.push_back(sample);
    }

    pub(super) fn prune_before(&mut self, cutoff_us: i64) {
        while self.samples.front().is_some_and(|sample| {
            sample
                .device_timestamp
                .unwrap_or(sample.system_timestamp)
                .saturating_sub(cutoff_us)
                < 0
        }) {
            let _ = self.samples.pop_front();
        }
    }
}

/// Returns true if the error indicates the trainer connection was lost.
pub(super) fn is_connection_lost_error(err: &Error) -> bool {
    matches!(err, Error::Ipc(IpcError::ConnectionLost))
}
