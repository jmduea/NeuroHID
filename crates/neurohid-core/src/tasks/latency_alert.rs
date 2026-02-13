use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, RwLock};
use tokio::time::MissedTickBehavior;

use neurohid_types::{config::LatencyAlertConfig, error::Result};

use crate::service::ServiceState;

/// Background monitor that triggers degraded-latency alerts when rolling p95
/// latency remains above thresholds for a sustained duration.
pub struct LatencyAlertMonitorTask {
    config: LatencyAlertConfig,
    state: Arc<RwLock<ServiceState>>,
}

impl LatencyAlertMonitorTask {
    pub fn new(config: LatencyAlertConfig, state: Arc<RwLock<ServiceState>>) -> Self {
        Self { config, state }
    }

    pub async fn run(self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        if !self.config.enabled {
            tracing::info!("Latency alert monitor disabled");
            return Ok(());
        }

        tracing::info!("Latency alert monitor started");

        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut tracker = LatencyAlertTracker::default();

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("Latency alert monitor received shutdown signal");
                    break;
                }
                _ = ticker.tick() => {
                    let (decode_p95, action_p95) = {
                        let state = self.state.read().await;
                        (state.decode_latency_p95_us, state.action_latency_p95_us)
                    };

                    let now = Instant::now();
                    let eval = tracker.evaluate(&self.config, decode_p95, action_p95, now);

                    if eval.changed || eval.degraded {
                        let mut state = self.state.write().await;
                        state.latency_degraded = eval.degraded;
                        state.latency_alert_message = eval.message.clone();
                    }

                    if eval.should_notify {
                        tracing::warn!(
                            decode_p95_us = decode_p95,
                            action_p95_us = action_p95,
                            decode_threshold_us = self.config.decode_p95_threshold_us,
                            action_threshold_us = self.config.action_p95_threshold_us,
                            "Latency thresholds exceeded for sustained duration"
                        );
                    } else if eval.became_healthy {
                        tracing::info!("Latency returned within configured thresholds");
                    }
                }
            }
        }

        {
            let mut state = self.state.write().await;
            state.latency_degraded = false;
            state.latency_alert_message = None;
        }

        tracing::info!("Latency alert monitor stopped");
        Ok(())
    }
}

#[derive(Default)]
struct LatencyAlertTracker {
    breach_started_at: Option<Instant>,
    last_notification_at: Option<Instant>,
    degraded: bool,
}

struct AlertEvaluation {
    degraded: bool,
    changed: bool,
    became_healthy: bool,
    should_notify: bool,
    message: Option<String>,
}

impl LatencyAlertTracker {
    fn evaluate(
        &mut self,
        config: &LatencyAlertConfig,
        decode_p95_us: u64,
        action_p95_us: u64,
        now: Instant,
    ) -> AlertEvaluation {
        let was_degraded = self.degraded;
        let decode_breach = decode_p95_us > config.decode_p95_threshold_us;
        let action_breach = action_p95_us > config.action_p95_threshold_us;
        let breached = decode_breach || action_breach;

        if !breached {
            self.breach_started_at = None;
            self.degraded = false;
            return AlertEvaluation {
                degraded: false,
                changed: was_degraded,
                became_healthy: was_degraded,
                should_notify: false,
                message: None,
            };
        }

        let breach_started_at = *self.breach_started_at.get_or_insert(now);
        let sustained_duration = Duration::from_secs(config.sustained_duration_secs.max(1));
        if now.saturating_duration_since(breach_started_at) < sustained_duration {
            self.degraded = false;
            return AlertEvaluation {
                degraded: false,
                changed: was_degraded,
                became_healthy: was_degraded,
                should_notify: false,
                message: None,
            };
        }

        self.degraded = true;
        let cooldown = Duration::from_secs(config.notification_cooldown_secs.max(1));
        let should_notify = match self.last_notification_at {
            Some(last) => now.saturating_duration_since(last) >= cooldown,
            None => true,
        };
        if should_notify {
            self.last_notification_at = Some(now);
        }

        let message = Some(format!(
            "Latency degraded: decode p95 {} us (>{}), action p95 {} us (>{})",
            decode_p95_us,
            config.decode_p95_threshold_us,
            action_p95_us,
            config.action_p95_threshold_us
        ));

        AlertEvaluation {
            degraded: true,
            changed: !was_degraded,
            became_healthy: false,
            should_notify,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use neurohid_types::config::LatencyAlertConfig;

    use super::LatencyAlertTracker;

    fn cfg() -> LatencyAlertConfig {
        LatencyAlertConfig {
            enabled: true,
            decode_p95_threshold_us: 10,
            action_p95_threshold_us: 20,
            sustained_duration_secs: 5,
            notification_cooldown_secs: 10,
        }
    }

    #[test]
    fn no_alert_before_sustained_duration() {
        let mut tracker = LatencyAlertTracker::default();
        let config = cfg();
        let t0 = Instant::now();

        let early = tracker.evaluate(&config, 50, 5, t0);
        assert!(!early.degraded);
        assert!(!early.should_notify);

        let still_early = tracker.evaluate(&config, 50, 5, t0 + Duration::from_secs(4));
        assert!(!still_early.degraded);
        assert!(!still_early.should_notify);
    }

    #[test]
    fn alert_after_sustained_duration_and_respects_cooldown() {
        let mut tracker = LatencyAlertTracker::default();
        let config = cfg();
        let t0 = Instant::now();

        let _ = tracker.evaluate(&config, 50, 5, t0);
        let first = tracker.evaluate(&config, 50, 5, t0 + Duration::from_secs(5));
        assert!(first.degraded);
        assert!(first.should_notify);

        let cooldown = tracker.evaluate(&config, 50, 5, t0 + Duration::from_secs(9));
        assert!(cooldown.degraded);
        assert!(!cooldown.should_notify);

        let second = tracker.evaluate(&config, 50, 5, t0 + Duration::from_secs(16));
        assert!(second.degraded);
        assert!(second.should_notify);
    }

    #[test]
    fn clears_when_latency_recovers() {
        let mut tracker = LatencyAlertTracker::default();
        let config = cfg();
        let t0 = Instant::now();

        let _ = tracker.evaluate(&config, 50, 5, t0);
        let _ = tracker.evaluate(&config, 50, 5, t0 + Duration::from_secs(5));
        let recovered = tracker.evaluate(&config, 1, 2, t0 + Duration::from_secs(6));
        assert!(!recovered.degraded);
        assert!(recovered.became_healthy);
        assert!(recovered.message.is_none());
    }
}
