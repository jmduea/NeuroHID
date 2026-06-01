//! Runtime observability gate.
//!
//! `EmitGate` provides cheap deterministic rate-limiting for hot-path log
//! statements in the task pipeline. Config types (`EmitPolicyConfig`,
//! `ObservabilityConfig`) remain in `neurohid-types::observability` because
//! they are part of the shared config schema.

use std::time::{Duration, Instant};

use neurohid_types::observability::EmitPolicyConfig;

const INFO_WINDOW_SECS: u64 = 60;
const DEBUG_WINDOW_SECS: u64 = 1;

#[derive(Debug)]
struct WindowCounter {
    started_at: Instant,
    count: u32,
}

impl WindowCounter {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
            count: 0,
        }
    }

    fn allow(&mut self, window: Duration, limit: u32) -> bool {
        if limit == 0 {
            return false;
        }

        if self.started_at.elapsed() >= window {
            self.started_at = Instant::now();
            self.count = 0;
        }

        if self.count >= limit {
            return false;
        }

        self.count = self.count.saturating_add(1);
        true
    }
}

/// Cheap deterministic emit gate for sampling and rate-limiting hot-path logs.
#[derive(Debug)]
pub struct EmitGate {
    policy: EmitPolicyConfig,
    info_counter: WindowCounter,
    debug_counter: WindowCounter,
    sample_seq: u64,
}

impl EmitGate {
    pub fn new(policy: EmitPolicyConfig) -> Self {
        Self {
            policy: policy.clamp(),
            info_counter: WindowCounter::new(),
            debug_counter: WindowCounter::new(),
            sample_seq: 0,
        }
    }

    pub fn allow_info(&mut self) -> bool {
        self.info_counter.allow(
            Duration::from_secs(INFO_WINDOW_SECS),
            self.policy.info_max_per_minute,
        )
    }

    pub fn allow_debug(&mut self) -> bool {
        if !self.sample_pass() {
            return false;
        }
        self.debug_counter.allow(
            Duration::from_secs(DEBUG_WINDOW_SECS),
            self.policy.debug_max_per_second,
        )
    }

    fn sample_pass(&mut self) -> bool {
        let ratio = self.policy.sample_ratio;
        if ratio <= 0.0 {
            return false;
        }
        if ratio >= 1.0 {
            return true;
        }

        self.sample_seq = self.sample_seq.wrapping_add(1);
        let stride = (1.0 / ratio).round().max(1.0) as u64;
        self.sample_seq.is_multiple_of(stride)
    }
}
