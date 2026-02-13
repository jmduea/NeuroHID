use std::collections::VecDeque;

/// Rolling latency tracker with p95 computation over a bounded window.
pub(crate) struct RollingLatency {
    samples: VecDeque<u64>,
    capacity: usize,
    last_us: u64,
    p95_us: u64,
}

impl RollingLatency {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity.max(1)),
            capacity: capacity.max(1),
            last_us: 0,
            p95_us: 0,
        }
    }

    pub(crate) fn record(&mut self, latency_us: u64) {
        self.last_us = latency_us;
        if self.samples.len() >= self.capacity {
            let _ = self.samples.pop_front();
        }
        self.samples.push_back(latency_us);
        self.p95_us = percentile_95(&self.samples);
    }

    pub(crate) fn last_us(&self) -> u64 {
        self.last_us
    }

    pub(crate) fn p95_us(&self) -> u64 {
        self.p95_us
    }
}

fn percentile_95(values: &VecDeque<u64>) -> u64 {
    if values.is_empty() {
        return 0;
    }

    let mut sorted: Vec<u64> = values.iter().copied().collect();
    sorted.sort_unstable();
    let idx = ((sorted.len() - 1) as f32 * 0.95).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::RollingLatency;

    #[test]
    fn rolling_latency_tracks_last_and_p95() {
        let mut latency = RollingLatency::new(5);
        for value in [10_u64, 20, 30, 40, 100] {
            latency.record(value);
        }
        assert_eq!(latency.last_us(), 100);
        assert_eq!(latency.p95_us(), 100);
    }

    #[test]
    fn rolling_latency_respects_window_capacity() {
        let mut latency = RollingLatency::new(3);
        latency.record(10);
        latency.record(20);
        latency.record(30);
        latency.record(40);
        assert_eq!(latency.last_us(), 40);
        assert_eq!(latency.p95_us(), 40);
    }
}
