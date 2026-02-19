//! Session and queue broker for unified IPC v3 channels.

use std::future::Future;
use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicU64, Ordering},
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore, broadcast};

use crate::protocol::{BrokerConfig, IpcChannel, QueueOverflowPolicy, RuntimeEvent};

/// Broker error variants.
#[derive(Debug, thiserror::Error)]
pub enum BrokerError {
    #[error("{channel:?} queue is full (capacity={capacity})")]
    QueueFull {
        channel: IpcChannel,
        capacity: usize,
    },
    #[error("trainer_busy: active session is '{active_session_id}'")]
    TrainerBusy { active_session_id: String },
    #[error("broker channel is closed for {channel:?}")]
    ChannelClosed { channel: IpcChannel },
    #[error("broker send failed for {channel:?}: {message}")]
    SendFailed {
        channel: IpcChannel,
        message: String,
    },
}

/// Snapshot of broker counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BrokerCounters {
    pub replay_hits: u64,
    pub replay_misses: u64,
    pub control_rejects: u64,
    pub trainer_queue_stalls: u64,
    pub runtime_backpressure_drops: u64,
    pub subscriber_lag_events: u64,
    pub connection_accepted: u64,
    pub connection_disconnected: u64,
}

#[derive(Debug, Default)]
struct CounterState {
    replay_hits: AtomicU64,
    replay_misses: AtomicU64,
    control_rejects: AtomicU64,
    trainer_queue_stalls: AtomicU64,
    runtime_backpressure_drops: AtomicU64,
    subscriber_lag_events: AtomicU64,
    connection_accepted: AtomicU64,
    connection_disconnected: AtomicU64,
}

/// One active trainer session guard.
#[derive(Debug)]
pub struct TrainerSessionGuard {
    active_session: Arc<StdMutex<Option<String>>>,
    session_id: String,
}

impl TrainerSessionGuard {
    /// Active trainer session id held by this guard.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

impl Drop for TrainerSessionGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active_session.lock()
            && active.as_deref() == Some(self.session_id.as_str())
        {
            *active = None;
        }
    }
}

/// IPC broker: queue policy enforcement plus trainer-session ownership.
#[derive(Debug, Clone)]
pub struct IpcBroker {
    config: BrokerConfig,
    control_permits: Arc<Semaphore>,
    trainer_permits: Arc<Semaphore>,
    active_trainer_session: Arc<StdMutex<Option<String>>>,
    runtime_event_tx: broadcast::Sender<RuntimeEvent>,
    counters: Arc<CounterState>,
}

impl IpcBroker {
    /// Build broker with queue policies from `config`.
    pub fn new(config: BrokerConfig) -> Self {
        let runtime_capacity = config.runtime_events.capacity.max(1);
        let (runtime_event_tx, _) = broadcast::channel(runtime_capacity);
        Self {
            control_permits: Arc::new(Semaphore::new(config.control.capacity.max(1))),
            trainer_permits: Arc::new(Semaphore::new(config.trainer.capacity.max(1))),
            active_trainer_session: Arc::new(StdMutex::new(None)),
            runtime_event_tx,
            counters: Arc::new(CounterState::default()),
            config,
        }
    }

    /// Return immutable broker policy configuration.
    pub const fn config(&self) -> &BrokerConfig {
        &self.config
    }

    /// Record accepted client connection churn metric.
    pub fn record_connection_accepted(&self) {
        self.counters
            .connection_accepted
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record disconnected client connection churn metric.
    pub fn record_connection_disconnected(&self) {
        self.counters
            .connection_disconnected
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Record replay hit metric.
    pub fn record_replay_hit(&self) {
        self.counters.replay_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record replay miss metric.
    pub fn record_replay_miss(&self) {
        self.counters.replay_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record runtime subscriber lag/drop metric.
    pub fn record_runtime_backpressure_drop(&self, dropped: u64) {
        self.counters
            .runtime_backpressure_drops
            .fetch_add(dropped, Ordering::Relaxed);
        self.counters
            .subscriber_lag_events
            .fetch_add(1, Ordering::Relaxed);
    }

    /// Return current broker metrics snapshot.
    pub fn counters(&self) -> BrokerCounters {
        BrokerCounters {
            replay_hits: self.counters.replay_hits.load(Ordering::Relaxed),
            replay_misses: self.counters.replay_misses.load(Ordering::Relaxed),
            control_rejects: self.counters.control_rejects.load(Ordering::Relaxed),
            trainer_queue_stalls: self.counters.trainer_queue_stalls.load(Ordering::Relaxed),
            runtime_backpressure_drops: self
                .counters
                .runtime_backpressure_drops
                .load(Ordering::Relaxed),
            subscriber_lag_events: self.counters.subscriber_lag_events.load(Ordering::Relaxed),
            connection_accepted: self.counters.connection_accepted.load(Ordering::Relaxed),
            connection_disconnected: self
                .counters
                .connection_disconnected
                .load(Ordering::Relaxed),
        }
    }

    /// Open one active trainer stream session, rejecting when busy.
    pub fn open_trainer_stream(
        &self,
        session_id: impl Into<String>,
    ) -> std::result::Result<TrainerSessionGuard, BrokerError> {
        let session_id = session_id.into();
        let mut active =
            self.active_trainer_session
                .lock()
                .map_err(|_| BrokerError::SendFailed {
                    channel: IpcChannel::TrainerStream,
                    message: "active trainer session lock poisoned".to_string(),
                })?;
        if let Some(existing) = active.as_ref() {
            return Err(BrokerError::TrainerBusy {
                active_session_id: existing.clone(),
            });
        }
        *active = Some(session_id.clone());
        Ok(TrainerSessionGuard {
            active_session: Arc::clone(&self.active_trainer_session),
            session_id,
        })
    }

    /// Return current active trainer session id.
    pub fn active_trainer_session(&self) -> Option<String> {
        self.active_trainer_session
            .lock()
            .ok()
            .and_then(|active| active.clone())
    }

    /// Enforce control queue policy around an async send operation.
    pub async fn send_control<T, E, F>(&self, send_op: F) -> std::result::Result<T, BrokerError>
    where
        E: std::fmt::Display,
        F: Future<Output = std::result::Result<T, E>>,
    {
        let _permit = self
            .acquire_permit(
                IpcChannel::ControlRpc,
                self.config.control,
                Arc::clone(&self.control_permits),
            )
            .await?;
        send_op.await.map_err(|error| BrokerError::SendFailed {
            channel: IpcChannel::ControlRpc,
            message: error.to_string(),
        })
    }

    /// Enforce trainer queue policy around an async send operation.
    pub async fn send_trainer<T, E, F>(&self, send_op: F) -> std::result::Result<T, BrokerError>
    where
        E: std::fmt::Display,
        F: Future<Output = std::result::Result<T, E>>,
    {
        let _permit = self
            .acquire_permit(
                IpcChannel::TrainerStream,
                self.config.trainer,
                Arc::clone(&self.trainer_permits),
            )
            .await?;
        send_op.await.map_err(|error| BrokerError::SendFailed {
            channel: IpcChannel::TrainerStream,
            message: error.to_string(),
        })
    }

    /// Subscribe to runtime events fan-out.
    pub fn subscribe_runtime_events(&self) -> broadcast::Receiver<RuntimeEvent> {
        self.runtime_event_tx.subscribe()
    }

    /// Publish one runtime event into broker fan-out.
    pub fn publish_runtime_event(
        &self,
        event: RuntimeEvent,
    ) -> std::result::Result<usize, BrokerError> {
        self.runtime_event_tx
            .send(event)
            .map_err(|_| BrokerError::ChannelClosed {
                channel: IpcChannel::RuntimeEvents,
            })
    }

    async fn acquire_permit(
        &self,
        channel: IpcChannel,
        policy: crate::protocol::ChannelPolicy,
        semaphore: Arc<Semaphore>,
    ) -> std::result::Result<OwnedSemaphorePermit, BrokerError> {
        match policy.overflow {
            QueueOverflowPolicy::RejectNew => semaphore.try_acquire_owned().map_err(|_| {
                if channel == IpcChannel::ControlRpc {
                    self.counters
                        .control_rejects
                        .fetch_add(1, Ordering::Relaxed);
                }
                BrokerError::QueueFull {
                    channel,
                    capacity: policy.capacity,
                }
            }),
            QueueOverflowPolicy::StallWarn => {
                if let Ok(permit) = semaphore.clone().try_acquire_owned() {
                    return Ok(permit);
                }
                if channel == IpcChannel::TrainerStream {
                    self.counters
                        .trainer_queue_stalls
                        .fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        capacity = policy.capacity,
                        "trainer.stream broker queue stalled; waiting for capacity"
                    );
                }
                semaphore
                    .acquire_owned()
                    .await
                    .map_err(|_| BrokerError::ChannelClosed { channel })
            }
            QueueOverflowPolicy::DropOldest => semaphore
                .acquire_owned()
                .await
                .map_err(|_| BrokerError::ChannelClosed { channel }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BrokerConfig, IpcBroker};

    #[test]
    fn trainer_stream_allows_single_active_session() {
        let broker = IpcBroker::new(BrokerConfig::default());
        let guard = broker
            .open_trainer_stream("trainer-1")
            .expect("first trainer session should open");
        let second = broker.open_trainer_stream("trainer-2");
        assert!(second.is_err());
        drop(guard);
        assert!(broker.open_trainer_stream("trainer-3").is_ok());
    }

    #[tokio::test]
    async fn control_reject_policy_tracks_rejects() {
        let mut config = BrokerConfig::default();
        config.control.capacity = 1;
        let broker = IpcBroker::new(config);

        let (hold_tx, hold_rx) = tokio::sync::oneshot::channel::<()>();
        let broker_clone = broker.clone();
        let task = tokio::spawn(async move {
            let _ = broker_clone
                .send_control(async move {
                    let _ = hold_rx.await;
                    Ok::<(), &'static str>(())
                })
                .await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let rejected = broker
            .send_control(async { Ok::<(), &'static str>(()) })
            .await;
        assert!(rejected.is_err());
        assert_eq!(broker.counters().control_rejects, 1);
        let _ = hold_tx.send(());
        let _ = task.await;
    }

    #[test]
    fn runtime_backpressure_drop_updates_counters() {
        let broker = IpcBroker::new(BrokerConfig::default());

        broker.record_runtime_backpressure_drop(5);

        let counters = broker.counters();
        assert_eq!(counters.runtime_backpressure_drops, 5);
        assert_eq!(counters.subscriber_lag_events, 1);
    }
}
