//! External runtime lifecycle and event streaming.

use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use tokio::sync::watch;

use neurohid_core::facade::{IpcClient, IpcConfig};
use neurohid_ipc::{IpcChannel, IpcEnvelope, RuntimeEvent, RuntimeEventsSubscribe};

use super::{
    EXTERNAL_EVENT_RECONNECT_BASE, EXTERNAL_EVENT_RECONNECT_MAX, ExternalEventState, ServiceManager,
};

impl ServiceManager {
    pub(super) fn start_external_event_worker(&mut self, runtime: &tokio::runtime::Runtime) {
        self.stop_external_event_worker();

        let endpoint = self.control_endpoint_label();
        let config = self.external_control_ipc_config();
        let state = Arc::clone(&self.external_event_state);
        let (stop_tx, stop_rx) = watch::channel(false);
        self.external_event_stop_tx = Some(stop_tx);
        self.external_event_task = Some(runtime.spawn(async move {
            run_external_event_worker(config, state, stop_rx).await;
            tracing::info!(endpoint = %endpoint, "external runtime.events worker stopped");
        }));
    }

    pub(super) fn stop_external_event_worker(&mut self) {
        if let Some(stop_tx) = self.external_event_stop_tx.take() {
            let _ = stop_tx.send(true);
        }
        if let Some(task) = self.external_event_task.take() {
            task.abort();
        }
        if let Ok(mut state) = self.external_event_state.lock() {
            state.stream_connected = false;
            state.last_event_at = None;
            state.last_error = None;
        }
    }

    pub(super) fn apply_external_runtime_event(
        state: &Arc<StdMutex<ExternalEventState>>,
        seq: u64,
        event: RuntimeEvent,
    ) {
        let Ok(mut stream_state) = state.lock() else {
            return;
        };
        stream_state.last_seq = Some(seq);
        stream_state.last_event_at = Some(Instant::now());
        stream_state.stream_connected = true;

        match event {
            RuntimeEvent::Snapshot { snapshot } => {
                stream_state.latest_snapshot = Some(snapshot);
                stream_state.last_error = None;
            }
            RuntimeEvent::TrainerSnapshot { snapshot } => {
                if let Some(cached) = stream_state.latest_snapshot.as_mut() {
                    cached.ml_bridge_connected = snapshot.trainer_connected;
                    cached.trainer_replay_size = Some(snapshot.replay_size);
                    cached.trainer_step = Some(snapshot.training_step);
                    cached.trainer_last_error = snapshot.last_error.clone();
                    cached.ml_protocol_version = snapshot.protocol_version;
                }
            }
            RuntimeEvent::TrainerStatus { status } => {
                if let Some(cached) = stream_state.latest_snapshot.as_mut() {
                    cached.trainer_replay_size = Some(status.replay_size);
                    cached.trainer_step = Some(status.training_step);
                    cached.trainer_policy_loss = status.policy_loss;
                    cached.trainer_value_loss = status.value_loss;
                    cached.trainer_entropy = status.entropy;
                    cached.trainer_last_error = status.last_error.clone();
                    cached.ml_bridge_connected = status.state != "disconnected";
                    cached.ml_bridge_stalled = status.state == "stalled";
                }
            }
            RuntimeEvent::RuntimeTelemetry { telemetry } => {
                if let Some(cached) = stream_state.latest_snapshot.as_mut() {
                    cached.signal_latency_p95_us = telemetry.signal_latency_p95_us;
                    cached.decode_latency_p95_us = telemetry.decode_latency_p95_us;
                    cached.action_latency_p95_us = telemetry.action_latency_p95_us;
                    cached.integrity_issue_count = telemetry.dropped_ml_messages;
                }
            }
            RuntimeEvent::Lifecycle { state, detail, .. } => {
                if state == "replay_miss" {
                    stream_state.replay_miss = true;
                    stream_state.last_error = Some(format!(
                        "runtime.events replay_miss; fallback snapshot polling enabled: {detail}"
                    ));
                } else if state == "replay_resumed" {
                    stream_state.replay_miss = false;
                    stream_state.last_error = None;
                }
            }
            _ => {}
        }
    }
}

async fn run_external_event_worker(
    config: IpcConfig,
    state: Arc<StdMutex<ExternalEventState>>,
    mut stop_rx: watch::Receiver<bool>,
) {
    let mut reconnect_delay = EXTERNAL_EVENT_RECONNECT_BASE;
    loop {
        if *stop_rx.borrow() {
            break;
        }

        let resume_from_seq = state
            .lock()
            .ok()
            .and_then(|guard| guard.last_seq.map(|seq| seq.saturating_add(1)));

        let mut client = IpcClient::new(config.clone());
        if let Err(error) = client.connect().await {
            set_external_event_stream_error(&state, format!("connect failed: {error}"));
            if sleep_external_reconnect(reconnect_delay, &mut stop_rx).await {
                break;
            }
            reconnect_delay = (reconnect_delay * 2).min(EXTERNAL_EVENT_RECONNECT_MAX);
            continue;
        }

        let subscribe = RuntimeEventsSubscribe {
            families: vec![
                "snapshot".to_string(),
                "trainer_snapshot".to_string(),
                "trainer_status".to_string(),
                "runtime_telemetry".to_string(),
                "decision_event".to_string(),
                "errp_window".to_string(),
                "errp_result".to_string(),
                "integrity_issue".to_string(),
                "lifecycle".to_string(),
            ],
            include_snapshot: true,
            include_capabilities: false,
            max_events: None,
            max_duration_ms: None,
            resume_from_seq,
            sample_every: 1,
            snapshot_interval_ms: 1_000,
        };
        let subscribe_envelope = match IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("hub-external-runtime-events".to_string()),
            &subscribe,
        ) {
            Ok(envelope) => envelope,
            Err(error) => {
                set_external_event_stream_error(
                    &state,
                    format!("failed to encode runtime.events subscribe envelope: {error}"),
                );
                let _ = client.disconnect().await;
                break;
            }
        };
        if let Err(error) = client.send(subscribe_envelope).await {
            set_external_event_stream_error(&state, format!("subscribe send failed: {error}"));
            let _ = client.disconnect().await;
            if sleep_external_reconnect(reconnect_delay, &mut stop_rx).await {
                break;
            }
            reconnect_delay = (reconnect_delay * 2).min(EXTERNAL_EVENT_RECONNECT_MAX);
            continue;
        }

        reconnect_delay = EXTERNAL_EVENT_RECONNECT_BASE;
        if let Ok(mut stream_state) = state.lock() {
            stream_state.stream_connected = true;
            stream_state.last_error = None;
        }

        loop {
            tokio::select! {
                changed = stop_rx.changed() => {
                    let stop_requested = changed.is_ok() && *stop_rx.borrow();
                    if stop_requested {
                        let _ = client.disconnect().await;
                        return;
                    }
                }
                incoming = client.recv() => {
                    let envelope = match incoming {
                        Ok(envelope) => envelope,
                        Err(error) => {
                            set_external_event_stream_error(
                                &state,
                                format!("stream receive failed: {error}"),
                            );
                            break;
                        }
                    };

                    if envelope.channel != IpcChannel::RuntimeEvents || envelope.msg_type != "event" {
                        continue;
                    }

                    let seq = envelope.seq;
                    match envelope.decode_payload::<RuntimeEvent>() {
                        Ok(event) => ServiceManager::apply_external_runtime_event(&state, seq, event),
                        Err(error) => {
                            set_external_event_stream_error(
                                &state,
                                format!("runtime.events payload decode failed: {error}"),
                            );
                        }
                    }
                }
            }
        }

        let _ = client.disconnect().await;
        if sleep_external_reconnect(reconnect_delay, &mut stop_rx).await {
            break;
        }
        reconnect_delay = (reconnect_delay * 2).min(EXTERNAL_EVENT_RECONNECT_MAX);
    }
}

async fn sleep_external_reconnect(delay: Duration, stop_rx: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(delay) => false,
        changed = stop_rx.changed() => changed.is_ok() && *stop_rx.borrow(),
    }
}

fn set_external_event_stream_error(state: &Arc<StdMutex<ExternalEventState>>, message: String) {
    if let Ok(mut stream_state) = state.lock() {
        stream_state.stream_connected = false;
        stream_state.last_error = Some(message);
    }
}
