mod events;

use std::sync::{
    Arc, Mutex as StdMutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use neurohid_core::observability::EmitGate;
use neurohid_core::runtime::RuntimeIpcHandle;
use neurohid_ipc::{
    BrokerConfig as RuntimeBrokerConfig, BrokerError as RuntimeBrokerError,
    IpcBroker as RuntimeIpcBroker, IpcConfig as RuntimeIpcConfig,
    IpcConnection as RuntimeIpcConnection, IpcServer as RuntimeIpcServer,
    IpcTransport as RuntimeIpcTransport,
};
use neurohid_ipc::{
    IPC_PROTOCOL_VERSION, IpcChannel, IpcEnvelope, RuntimeEvent, RuntimeEventsSubscribe,
};
use neurohid_types::{
    config::IpcMode,
    control::{ControlCommand, ControlRequest, ControlResponse},
    observability::{self as obs, EmitPolicyConfig},
};
use tokio::sync::{Mutex, broadcast};

use events::{
    RuntimeEventsFilter, RuntimeEventsReplayItem, RuntimeEventsState, RuntimeObservationState,
    build_runtime_capabilities_event, build_runtime_telemetry, build_runtime_trainer_status,
    runtime_event_family,
};

/// Default TCP port for the control server when running standalone with no config endpoint.
pub(crate) const DEFAULT_STANDALONE_CONTROL_PORT: u16 = 47384;
/// Default control endpoint string used by CLI when --endpoint is not set.
pub(crate) const DEFAULT_CONTROL_ENDPOINT: &str = "127.0.0.1:47384";

#[derive(Debug, Default)]
struct ConnectionChurnState {
    accepted: u64,
    active: u64,
    disconnected: u64,
}

pub(crate) fn resolve_runtime_ipc_server_config(
    service_config: &neurohid_types::config::ServiceConfig,
    control_port: Option<u16>,
) -> anyhow::Result<Option<RuntimeIpcConfig>> {
    if let Some(port) = control_port {
        return Ok(Some(RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        }));
    }

    let (transport, endpoint) = match service_config.ipc_mode {
        IpcMode::LocalSocket => (
            RuntimeIpcTransport::LocalSocket,
            service_config.ipc_endpoint.clone(),
        ),
        IpcMode::TcpLoopback => (
            RuntimeIpcTransport::TcpLoopback,
            service_config.ipc_endpoint.clone(),
        ),
    };

    if endpoint.trim().is_empty() {
        return Ok(None);
    }

    validate_local_only_endpoint(transport, &endpoint)?;

    Ok(Some(RuntimeIpcConfig {
        transport,
        endpoint,
        ..RuntimeIpcConfig::default()
    }))
}

pub(crate) fn validate_local_only_endpoint(
    transport: RuntimeIpcTransport,
    endpoint: &str,
) -> anyhow::Result<()> {
    use std::net::ToSocketAddrs;

    if transport == RuntimeIpcTransport::LocalSocket {
        if endpoint.trim().is_empty() {
            return Err(anyhow::anyhow!(
                "local_socket endpoint must not be empty for IPC server"
            ));
        }
        return Ok(());
    }

    let mut addrs = endpoint.to_socket_addrs().map_err(|error| {
        anyhow::anyhow!("invalid tcp_loopback endpoint '{}': {}", endpoint, error)
    })?;
    let mut resolved_any = false;
    for addr in addrs.by_ref() {
        resolved_any = true;
        if !addr.ip().is_loopback() {
            return Err(anyhow::anyhow!(
                "non-loopback IPC endpoint '{}' is not allowed (resolved {})",
                endpoint,
                addr
            ));
        }
    }
    if !resolved_any {
        return Err(anyhow::anyhow!(
            "tcp_loopback endpoint '{}' did not resolve to any address",
            endpoint
        ));
    }
    Ok(())
}

pub(crate) async fn run_ipc_control_server(
    server_config: RuntimeIpcConfig,
    runtime: RuntimeIpcHandle,
    control_policy: EmitPolicyConfig,
) -> anyhow::Result<()> {
    let server = RuntimeIpcServer::new(server_config)
        .await
        .map_err(|error| anyhow::anyhow!("Failed to start IPC server: {}", error))?;
    let broker = Arc::new(RuntimeIpcBroker::new(RuntimeBrokerConfig::default()));
    {
        let mut runtime_bridge_rx = runtime.subscribe_runtime_bridge_events();
        let bridge_broker = Arc::clone(&broker);
        tokio::spawn(async move {
            loop {
                match runtime_bridge_rx.recv().await {
                    Ok(event) => {
                        let _ = bridge_broker.publish_runtime_event(event);
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        bridge_broker.record_runtime_backpressure_drop(skipped);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }
    let control_gate = Arc::new(StdMutex::new(EmitGate::new(control_policy)));
    let runtime_events_state = Arc::new(Mutex::new(RuntimeEventsState::default()));
    let churn_state = Arc::new(Mutex::new(ConnectionChurnState::default()));
    let next_connection_id = AtomicU64::new(1);

    loop {
        let connection = server
            .accept()
            .await
            .map_err(|error| anyhow::anyhow!("IPC accept failed: {}", error))?;
        let connection_id = next_connection_id.fetch_add(1, Ordering::Relaxed);
        broker.record_connection_accepted();
        {
            let mut churn = churn_state.lock().await;
            churn.accepted = churn.accepted.saturating_add(1);
            churn.active = churn.active.saturating_add(1);
            tracing::info!(
                connection_id,
                accepted = churn.accepted,
                active = churn.active,
                disconnected = churn.disconnected,
                "IPC client connected"
            );
        }

        let runtime_for_task = runtime.clone();
        let broker_for_task = Arc::clone(&broker);
        let control_gate_for_task = Arc::clone(&control_gate);
        let runtime_events_state_for_task = Arc::clone(&runtime_events_state);
        let churn_for_task = Arc::clone(&churn_state);
        tokio::spawn(async move {
            let result = handle_ipc_client_connection(
                connection_id,
                connection,
                runtime_for_task,
                Arc::clone(&broker_for_task),
                control_gate_for_task,
                runtime_events_state_for_task,
            )
            .await;
            broker_for_task.record_connection_disconnected();
            let counters = broker_for_task.counters();

            let mut churn = churn_for_task.lock().await;
            churn.active = churn.active.saturating_sub(1);
            churn.disconnected = churn.disconnected.saturating_add(1);
            match result {
                Ok(()) => tracing::info!(
                    connection_id,
                    accepted = churn.accepted,
                    active = churn.active,
                    disconnected = churn.disconnected,
                    replay_hits = counters.replay_hits,
                    replay_misses = counters.replay_misses,
                    control_rejects = counters.control_rejects,
                    trainer_queue_stalls = counters.trainer_queue_stalls,
                    runtime_backpressure_drops = counters.runtime_backpressure_drops,
                    subscriber_lag_events = counters.subscriber_lag_events,
                    "IPC client disconnected"
                ),
                Err(error) => tracing::warn!(
                    connection_id,
                    accepted = churn.accepted,
                    active = churn.active,
                    disconnected = churn.disconnected,
                    replay_hits = counters.replay_hits,
                    replay_misses = counters.replay_misses,
                    control_rejects = counters.control_rejects,
                    trainer_queue_stalls = counters.trainer_queue_stalls,
                    runtime_backpressure_drops = counters.runtime_backpressure_drops,
                    subscriber_lag_events = counters.subscriber_lag_events,
                    "IPC client disconnected with error: {}",
                    error
                ),
            }
        });
    }
}

async fn handle_ipc_client_connection(
    connection_id: u64,
    connection: RuntimeIpcConnection,
    runtime: RuntimeIpcHandle,
    broker: Arc<RuntimeIpcBroker>,
    control_gate: Arc<StdMutex<EmitGate>>,
    runtime_events_state: Arc<Mutex<RuntimeEventsState>>,
) -> anyhow::Result<()> {
    let mut control_response_seq = 0_u64;

    loop {
        let request_envelope = match connection.recv().await {
            Ok(envelope) => envelope,
            Err(error) => {
                tracing::debug!("IPC receive loop terminated: {}", error);
                break;
            }
        };

        if request_envelope.channel == IpcChannel::RuntimeEvents
            && request_envelope.msg_type == "subscribe"
        {
            handle_runtime_events_subscription(
                &connection,
                &runtime,
                &broker,
                request_envelope,
                runtime_events_state,
            )
            .await?;
            break;
        }

        if request_envelope.channel == IpcChannel::TrainerStream {
            handle_trainer_stream_connection(
                connection_id,
                &connection,
                &runtime,
                &broker,
                request_envelope,
            )
            .await?;
            break;
        }

        let (response_envelope, should_shutdown) = handle_control_request_envelope(
            request_envelope,
            &runtime,
            &control_gate,
            &mut control_response_seq,
        )
        .await;
        match broker
            .send_control(connection.send(response_envelope))
            .await
        {
            Ok(()) => {}
            Err(RuntimeBrokerError::QueueFull { .. }) => {
                control_response_seq = control_response_seq.saturating_add(1);
                let queue_full_envelope = IpcEnvelope {
                    v: IPC_PROTOCOL_VERSION,
                    channel: IpcChannel::ControlRpc,
                    msg_type: "error".to_string(),
                    seq: control_response_seq,
                    request_id: None,
                    session_id: Some("runtime-control".to_string()),
                    sent_at_us: neurohid_types::now_micros(),
                    payload: serde_json::json!({
                        "code": "control_queue_full",
                        "message": "control.rpc queue is full; request rejected",
                    }),
                };
                connection
                    .send(queue_full_envelope)
                    .await
                    .map_err(|error| {
                        anyhow::anyhow!(
                            "failed to send control queue-full response envelope: {}",
                            error
                        )
                    })?;
            }
            Err(error) => {
                return Err(anyhow::anyhow!(
                    "failed to send control response through broker: {}",
                    error
                ));
            }
        }
        if should_shutdown {
            tracing::info!(connection_id, "shutdown request processed via control.rpc");
            break;
        }
    }

    Ok(())
}

async fn handle_trainer_stream_connection(
    connection_id: u64,
    connection: &RuntimeIpcConnection,
    runtime: &RuntimeIpcHandle,
    broker: &RuntimeIpcBroker,
    first_envelope: IpcEnvelope,
) -> anyhow::Result<()> {
    if first_envelope.channel != IpcChannel::TrainerStream {
        return Err(anyhow::anyhow!(
            "trainer bridge received non-trainer first envelope"
        ));
    }

    let requested_session_id = first_envelope
        .session_id
        .clone()
        .unwrap_or_else(|| format!("trainer-{}", neurohid_types::now_micros()));
    let trainer_guard = match broker.open_trainer_stream(requested_session_id.clone()) {
        Ok(guard) => guard,
        Err(RuntimeBrokerError::TrainerBusy { active_session_id }) => {
            let busy = IpcEnvelope {
                v: IPC_PROTOCOL_VERSION,
                channel: IpcChannel::TrainerStream,
                msg_type: "error".to_string(),
                seq: first_envelope.seq.saturating_add(1),
                request_id: first_envelope.request_id.clone(),
                session_id: Some(requested_session_id),
                sent_at_us: neurohid_types::now_micros(),
                payload: serde_json::json!({
                    "code": "trainer_busy",
                    "message": "trainer.stream already has an active session",
                    "active_session_id": active_session_id,
                }),
            };
            connection.send(busy).await.map_err(|error| {
                anyhow::anyhow!("failed to send trainer_busy envelope to client: {}", error)
            })?;
            return Ok(());
        }
        Err(error) => {
            return Err(anyhow::anyhow!(
                "failed to open trainer stream in broker: {}",
                error
            ));
        }
    };

    runtime
        .trainer_connected(trainer_guard.session_id().to_string())
        .await
        .map_err(|error| anyhow::anyhow!("failed to notify runtime trainer connect: {}", error))?;
    runtime
        .trainer_send_envelope(first_envelope)
        .await
        .map_err(|error| {
            anyhow::anyhow!("failed to forward initial trainer envelope: {}", error)
        })?;
    tracing::info!(
        connection_id,
        session_id = trainer_guard.session_id(),
        "trainer.stream session started"
    );

    let relay_result: anyhow::Result<()> = loop {
        tokio::select! {
            incoming = connection.recv() => {
                let envelope = match incoming {
                    Ok(envelope) => envelope,
                    Err(_) => break Ok(()),
                };
                if envelope.channel != IpcChannel::TrainerStream {
                    break Err(anyhow::anyhow!(
                        "trainer stream received mixed channel {:?}; only trainer.stream is allowed",
                        envelope.channel
                    ));
                }
                runtime.trainer_send_envelope(envelope).await.map_err(|error| {
                    anyhow::anyhow!("failed to forward trainer.stream envelope to runtime: {}", error)
                })?;
            }
            outbound = runtime.recv_trainer_envelope() => {
                let envelope = match outbound {
                    Some(envelope) => envelope,
                    None => break Ok(()),
                };
                broker.send_trainer(connection.send(envelope)).await.map_err(|error| {
                    anyhow::anyhow!("failed to forward trainer.stream envelope to client: {}", error)
                })?;
            }
        }
    };

    let disconnect_result = runtime
        .trainer_disconnected()
        .await
        .map_err(|error| anyhow::anyhow!("failed to notify runtime trainer disconnect: {}", error));
    drop(trainer_guard);
    tracing::info!(connection_id, "trainer.stream session closed");
    relay_result?;
    disconnect_result?;
    Ok(())
}

async fn handle_control_request_envelope(
    envelope: IpcEnvelope,
    runtime: &RuntimeIpcHandle,
    control_gate: &StdMutex<EmitGate>,
    response_seq: &mut u64,
) -> (IpcEnvelope, bool) {
    let request_id = envelope.request_id.clone();
    let started = Instant::now();
    *response_seq = response_seq.saturating_add(1);

    if envelope.channel == IpcChannel::ControlRpc {
        let request_payload = if envelope.msg_type == "request" {
            envelope
                .decode_payload::<ControlRequest>()
                .map_err(|e| format!("invalid control request payload: {}", e))
        } else {
            Err("invalid control envelope channel/msg_type".to_string())
        };

        let (response, should_shutdown) = match request_payload {
            Ok(request) => {
                let command = control_command_name(&request.command);
                let _request_span = tracing::debug_span!(
                    obs::span::CONTROL_REQUEST,
                    stage = obs::stage::CONTROL,
                    request_id = request_id.as_deref().unwrap_or("none"),
                    command,
                    decision_id = obs::field::UNKNOWN,
                    stream_id = obs::field::UNKNOWN
                )
                .entered();
                if tracing::enabled!(tracing::Level::DEBUG) && gate_allows_debug(control_gate) {
                    tracing::debug!(
                        event = obs::event::CONTROL_REQUEST_RECEIVED,
                        request_id = request_id.as_deref().unwrap_or("none"),
                        decision_id = obs::field::UNKNOWN,
                        stream_id = obs::field::UNKNOWN,
                        command,
                        "Control request received"
                    );
                }
                let should_shutdown = matches!(request.command, ControlCommand::Shutdown);
                drop(_request_span);
                let response = runtime.dispatch_control_request(request).await;
                if tracing::enabled!(tracing::Level::DEBUG) && gate_allows_debug(control_gate) {
                    tracing::debug!(
                        event = obs::event::CONTROL_RESPONSE_SENT,
                        request_id = request_id.as_deref().unwrap_or("none"),
                        decision_id = obs::field::UNKNOWN,
                        stream_id = obs::field::UNKNOWN,
                        command,
                        duration_ms = started.elapsed().as_millis() as u64,
                        "Control request handled"
                    );
                }
                (response, should_shutdown)
            }
            Err(error) => (ControlResponse::error(request_id.clone(), error), false),
        };

        let response_v3 = response;
        let envelope = IpcEnvelope::new(
            IpcChannel::ControlRpc,
            "response",
            *response_seq,
            request_id,
            Some("runtime-control".to_string()),
            &response_v3,
        )
        .unwrap_or_else(|error| IpcEnvelope {
            v: IPC_PROTOCOL_VERSION,
            channel: IpcChannel::ControlRpc,
            msg_type: "response".to_string(),
            seq: *response_seq,
            request_id: None,
            session_id: Some("runtime-control".to_string()),
            sent_at_us: neurohid_types::now_micros(),
            payload: serde_json::json!({
                "request_id": null,
                "type": "error",
                "message": format!("failed to encode control response envelope: {}", error),
            }),
        });

        return (envelope, should_shutdown);
    }

    if envelope.channel == IpcChannel::RuntimeEvents
        && matches!(envelope.msg_type.as_str(), "poll" | "request")
    {
        let family = envelope
            .payload
            .get("family")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("snapshot");
        let snapshot = runtime.snapshot();
        let event = match family {
            "snapshot" => RuntimeEvent::Snapshot {
                snapshot: snapshot.clone(),
            },
            "trainer_snapshot" => RuntimeEvent::TrainerSnapshot {
                snapshot: runtime.trainer_snapshot(),
            },
            "trainer_status" => RuntimeEvent::TrainerStatus {
                status: build_runtime_trainer_status(&snapshot),
            },
            "runtime_telemetry" => RuntimeEvent::RuntimeTelemetry {
                telemetry: build_runtime_telemetry(&snapshot),
            },
            "capabilities" => build_runtime_capabilities_event(&snapshot),
            other => RuntimeEvent::Lifecycle {
                state: "error".to_string(),
                detail: format!("unsupported runtime.events family '{}'", other),
                requested_seq: None,
                replay_window_start_seq: None,
                replay_window_end_seq: None,
            },
        };
        let response = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "event",
            *response_seq,
            request_id,
            Some("runtime-events".to_string()),
            &event,
        )
        .unwrap_or_else(|error| IpcEnvelope {
            v: IPC_PROTOCOL_VERSION,
            channel: IpcChannel::RuntimeEvents,
            msg_type: "error".to_string(),
            seq: *response_seq,
            request_id: None,
            session_id: Some("runtime-events".to_string()),
            sent_at_us: neurohid_types::now_micros(),
            payload: serde_json::json!({
                "message": format!("failed to encode runtime event envelope: {}", error),
            }),
        });
        return (response, false);
    }

    let error = IpcEnvelope {
        v: IPC_PROTOCOL_VERSION,
        channel: envelope.channel,
        msg_type: "error".to_string(),
        seq: *response_seq,
        request_id,
        session_id: Some("runtime-control".to_string()),
        sent_at_us: neurohid_types::now_micros(),
        payload: serde_json::json!({
            "message": "unsupported channel/msg_type",
        }),
    };
    (error, false)
}

fn gate_allows_debug(control_gate: &StdMutex<EmitGate>) -> bool {
    match control_gate.lock() {
        Ok(mut gate) => gate.allow_debug(),
        Err(poisoned) => poisoned.into_inner().allow_debug(),
    }
}

async fn handle_runtime_events_subscription(
    connection: &RuntimeIpcConnection,
    runtime: &RuntimeIpcHandle,
    broker: &RuntimeIpcBroker,
    envelope: IpcEnvelope,
    runtime_events_state: Arc<Mutex<RuntimeEventsState>>,
) -> anyhow::Result<()> {
    let request = serde_json::from_value::<RuntimeEventsSubscribe>(envelope.payload.clone())
        .unwrap_or_default();
    let filter = RuntimeEventsFilter::from_request(&request);
    let request_id = envelope.request_id.clone();
    let session_id = envelope
        .session_id
        .clone()
        .unwrap_or_else(|| "runtime-events".to_string());
    {
        let mut state = runtime_events_state.lock().await;
        state.replay.prune();
    }

    let mut emitted = 0_u64;
    let max_events = request.max_events.unwrap_or(u64::MAX);
    let max_duration = request.max_duration_ms.map(Duration::from_millis);
    let sample_every = request.sample_every.max(1);
    let snapshot_interval_ms = request.snapshot_interval_ms.max(100);
    let started = Instant::now();

    let mut sample_rx = runtime.subscribe_samples();
    let mut feature_rx = runtime.subscribe_features();
    let mut action_rx = runtime.subscribe_actions();
    let mut marker_rx = runtime.subscribe_markers();
    let mut broker_event_rx = broker.subscribe_runtime_events();
    let mut snapshot_tick = tokio::time::interval(Duration::from_millis(snapshot_interval_ms));
    snapshot_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut observation_state = RuntimeObservationState::default();
    let mut sampled_frames = 0_u64;

    if let Some(resume_from_seq) = request.resume_from_seq {
        let (oldest_seq, newest_seq, replay_items) = {
            let state = runtime_events_state.lock().await;
            let oldest_seq = state.replay.oldest_seq();
            let newest_seq = state.replay.newest_seq();
            let replay_items = oldest_seq
                .zip(newest_seq)
                .is_some_and(|(oldest, newest)| {
                    resume_from_seq >= oldest && resume_from_seq <= newest.saturating_add(1)
                })
                .then(|| {
                    state
                        .replay
                        .iter_from(resume_from_seq)
                        .cloned()
                        .collect::<Vec<_>>()
                });
            (oldest_seq, newest_seq, replay_items)
        };
        let replay_hit = oldest_seq.zip(newest_seq).is_some_and(|(oldest, newest)| {
            resume_from_seq >= oldest && resume_from_seq <= newest.saturating_add(1)
        });

        if replay_hit {
            broker.record_replay_hit();
            for item in replay_items.unwrap_or_default() {
                if emitted >= max_events {
                    break;
                }
                emit_runtime_event_replay(
                    connection,
                    &request_id,
                    &session_id,
                    &item,
                    &filter,
                    &mut emitted,
                )
                .await?;
            }
            emit_runtime_event(
                connection,
                &runtime_events_state,
                &request_id,
                &session_id,
                RuntimeEvent::Lifecycle {
                    state: "replay_resumed".to_string(),
                    detail: format!("resumed from seq {}", resume_from_seq),
                    requested_seq: Some(resume_from_seq),
                    replay_window_start_seq: oldest_seq,
                    replay_window_end_seq: newest_seq,
                },
                &filter,
                &mut emitted,
            )
            .await?;
        } else {
            broker.record_replay_miss();
            let detail = match (oldest_seq, newest_seq) {
                (Some(oldest), Some(newest)) => {
                    format!(
                        "requested seq {} outside replay window {}..={}",
                        resume_from_seq, oldest, newest
                    )
                }
                _ => format!(
                    "requested seq {} but replay buffer is empty",
                    resume_from_seq
                ),
            };
            emit_runtime_event(
                connection,
                &runtime_events_state,
                &request_id,
                &session_id,
                RuntimeEvent::Lifecycle {
                    state: "replay_miss".to_string(),
                    detail,
                    requested_seq: Some(resume_from_seq),
                    replay_window_start_seq: oldest_seq,
                    replay_window_end_seq: newest_seq,
                },
                &filter,
                &mut emitted,
            )
            .await?;
        }
    }

    if request.include_capabilities {
        emit_runtime_event(
            connection,
            &runtime_events_state,
            &request_id,
            &session_id,
            build_runtime_capabilities_event(&runtime.snapshot()),
            &filter,
            &mut emitted,
        )
        .await?;
    }

    if request.include_snapshot {
        let snapshot = runtime.snapshot();
        emit_runtime_event(
            connection,
            &runtime_events_state,
            &request_id,
            &session_id,
            RuntimeEvent::Snapshot {
                snapshot: snapshot.clone(),
            },
            &filter,
            &mut emitted,
        )
        .await?;
        emit_runtime_event(
            connection,
            &runtime_events_state,
            &request_id,
            &session_id,
            RuntimeEvent::TrainerSnapshot {
                snapshot: runtime.trainer_snapshot(),
            },
            &filter,
            &mut emitted,
        )
        .await?;
    }

    while emitted < max_events {
        if max_duration.is_some_and(|duration| started.elapsed() >= duration) {
            break;
        }

        tokio::select! {
            sample = sample_rx.recv() => {
                match sample {
                    Ok(sample) => {
                        sampled_frames = sampled_frames.saturating_add(1);
                        if sampled_frames.is_multiple_of(sample_every) {
                            emit_runtime_event(
                                connection,
                                &runtime_events_state,
                                &request_id,
                                &session_id,
                                RuntimeEvent::Sample { sample },
                                &filter,
                                &mut emitted,
                            )
                            .await?;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        broker.record_runtime_backpressure_drop(skipped);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEvent::BackpressureDrop {
                                channel: IpcChannel::RuntimeEvents,
                                dropped: skipped,
                                reason: "sample stream lagged".to_string(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            feature = feature_rx.recv() => {
                match feature {
                    Ok(feature) => {
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEvent::FeatureFrame {
                                feature: feature.clone(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                        let observation = observation_state.observation_from_feature(&feature);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEvent::ObservationFrame { observation },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        broker.record_runtime_backpressure_drop(skipped);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEvent::BackpressureDrop {
                                channel: IpcChannel::RuntimeEvents,
                                dropped: skipped,
                                reason: "feature stream lagged".to_string(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            action = action_rx.recv() => {
                match action {
                    Ok(action) => {
                        observation_state.update_from_action(&action);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEvent::ActionEmitted { action },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        broker.record_runtime_backpressure_drop(skipped);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEvent::BackpressureDrop {
                                channel: IpcChannel::RuntimeEvents,
                                dropped: skipped,
                                reason: "action stream lagged".to_string(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            marker = marker_rx.recv() => {
                match marker {
                    Ok(marker) => {
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEvent::Marker { marker },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        broker.record_runtime_backpressure_drop(skipped);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEvent::BackpressureDrop {
                                channel: IpcChannel::RuntimeEvents,
                                dropped: skipped,
                                reason: "marker stream lagged".to_string(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            broker_event = broker_event_rx.recv() => {
                match broker_event {
                    Ok(event) => {
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            event,
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        broker.record_runtime_backpressure_drop(skipped);
                        emit_runtime_event(
                            connection,
                            &runtime_events_state,
                            &request_id,
                            &session_id,
                            RuntimeEvent::BackpressureDrop {
                                channel: IpcChannel::RuntimeEvents,
                                dropped: skipped,
                                reason: "runtime.events broker subscriber lagged".to_string(),
                            },
                            &filter,
                            &mut emitted,
                        )
                        .await?;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = snapshot_tick.tick() => {
                let snapshot = runtime.snapshot();
                emit_runtime_event(
                    connection,
                    &runtime_events_state,
                    &request_id,
                    &session_id,
                    RuntimeEvent::Snapshot {
                        snapshot: snapshot.clone(),
                    },
                    &filter,
                    &mut emitted,
                )
                .await?;
                emit_runtime_event(
                    connection,
                    &runtime_events_state,
                    &request_id,
                    &session_id,
                    RuntimeEvent::TrainerStatus {
                        status: build_runtime_trainer_status(&snapshot),
                    },
                    &filter,
                    &mut emitted,
                )
                .await?;
                emit_runtime_event(
                    connection,
                    &runtime_events_state,
                    &request_id,
                    &session_id,
                    RuntimeEvent::RuntimeTelemetry {
                        telemetry: build_runtime_telemetry(&snapshot),
                    },
                    &filter,
                    &mut emitted,
                )
                .await?;
            }
        }
    }

    let close_detail = if emitted >= max_events {
        "max_events_reached"
    } else if max_duration.is_some_and(|duration| started.elapsed() >= duration) {
        "max_duration_reached"
    } else {
        "stream_closed"
    };
    emit_runtime_event(
        connection,
        &runtime_events_state,
        &request_id,
        &session_id,
        RuntimeEvent::Lifecycle {
            state: "subscription_closed".to_string(),
            detail: close_detail.to_string(),
            requested_seq: None,
            replay_window_start_seq: None,
            replay_window_end_seq: None,
        },
        &filter,
        &mut emitted,
    )
    .await?;

    Ok(())
}

async fn emit_runtime_event(
    connection: &RuntimeIpcConnection,
    runtime_events_state: &Arc<Mutex<RuntimeEventsState>>,
    request_id: &Option<String>,
    session_id: &str,
    event: RuntimeEvent,
    filter: &RuntimeEventsFilter,
    emitted: &mut u64,
) -> anyhow::Result<()> {
    let family = runtime_event_family(&event);
    let sent_at_us = neurohid_types::now_micros();
    let payload = serde_json::to_value(&event)
        .map_err(|error| anyhow::anyhow!("failed to encode runtime event payload: {}", error))?;
    let seq = {
        let mut state = runtime_events_state.lock().await;
        let seq = state.allocate_seq();
        state.replay.push(RuntimeEventsReplayItem {
            seq,
            sent_at_us,
            family,
            event,
        });
        seq
    };

    if family != "lifecycle" && !filter.allows(family) {
        return Ok(());
    }

    let envelope = IpcEnvelope {
        v: IPC_PROTOCOL_VERSION,
        channel: IpcChannel::RuntimeEvents,
        msg_type: "event".to_string(),
        seq,
        request_id: request_id.clone(),
        session_id: Some(session_id.to_string()),
        sent_at_us,
        payload,
    };
    connection
        .send(envelope)
        .await
        .map_err(|error| anyhow::anyhow!("failed to send runtime.events envelope: {}", error))?;
    *emitted = emitted.saturating_add(1);
    Ok(())
}

async fn emit_runtime_event_replay(
    connection: &RuntimeIpcConnection,
    request_id: &Option<String>,
    session_id: &str,
    item: &RuntimeEventsReplayItem,
    filter: &RuntimeEventsFilter,
    emitted: &mut u64,
) -> anyhow::Result<()> {
    if item.family != "lifecycle" && !filter.allows(item.family) {
        return Ok(());
    }

    let payload = serde_json::to_value(&item.event).map_err(|error| {
        anyhow::anyhow!("failed to encode replay runtime event payload: {}", error)
    })?;
    let envelope = IpcEnvelope {
        v: IPC_PROTOCOL_VERSION,
        channel: IpcChannel::RuntimeEvents,
        msg_type: "event".to_string(),
        seq: item.seq,
        request_id: request_id.clone(),
        session_id: Some(session_id.to_string()),
        sent_at_us: item.sent_at_us,
        payload,
    };
    connection.send(envelope).await.map_err(|error| {
        anyhow::anyhow!(
            "failed to send replay runtime.events envelope for seq {}: {}",
            item.seq,
            error
        )
    })?;
    *emitted = emitted.saturating_add(1);
    Ok(())
}

fn control_command_name(command: &ControlCommand) -> &'static str {
    match command {
        ControlCommand::Snapshot => "snapshot",
        ControlCommand::Shutdown => "shutdown",
        ControlCommand::SetCalibrationMode { .. } => "set_calibration_mode",
        ControlCommand::SetOutputEnabled { .. } => "set_output_enabled",
        ControlCommand::ReloadModel => "reload_model",
        ControlCommand::PromoteCandidateModel => "promote_candidate_model",
        ControlCommand::RescanStreams => "rescan_streams",
        ControlCommand::ConnectStream { .. } => "connect_stream",
        ControlCommand::DisconnectStream { .. } => "disconnect_stream",
        ControlCommand::SetLearningEnabled { .. } => "set_learning_enabled",
        ControlCommand::MlBridgeReconnect => "ml_bridge_reconnect",
        ControlCommand::TrainerSnapshot => "trainer_snapshot",
        ControlCommand::SetFallbackPolicy { .. } => "set_fallback_policy",
        ControlCommand::SetSignalConfig { .. } => "set_signal_config",
        ControlCommand::StartRecording { .. } => "start_recording",
        ControlCommand::StopRecording => "stop_recording",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use events::{
        RUNTIME_EVENTS_REPLAY_MAX_EVENTS, RUNTIME_EVENTS_REPLAY_RETENTION_US,
        RuntimeEventsReplayBuffer,
    };
    use neurohid_core::runtime::{RuntimeBuilder, RuntimeCommand, RuntimeHandle};
    use neurohid_ipc::IpcClient as RuntimeIpcClient;
    use neurohid_types::{
        config::{BrainFlowConfig, DeviceBackend, SystemConfig},
        control::{
            ControlCommand, ControlRequest, ControlResponsePayload, ControlSnapshot,
            RuntimeModeState,
        },
    };

    fn replay_item(seq: u64, sent_at_us: i64) -> RuntimeEventsReplayItem {
        RuntimeEventsReplayItem {
            seq,
            sent_at_us,
            family: "lifecycle",
            event: RuntimeEvent::Lifecycle {
                state: "test".to_string(),
                detail: "test".to_string(),
                requested_seq: None,
                replay_window_start_seq: None,
                replay_window_end_seq: None,
            },
        }
    }

    #[test]
    fn replay_buffer_prunes_oldest_when_over_capacity() {
        let now = neurohid_types::now_micros();
        let mut replay = RuntimeEventsReplayBuffer::default();
        let total = RUNTIME_EVENTS_REPLAY_MAX_EVENTS as u64 + 5;
        for seq in 1..=total {
            replay.push(replay_item(seq, now));
        }

        assert_eq!(replay.len(), RUNTIME_EVENTS_REPLAY_MAX_EVENTS);
        assert_eq!(replay.oldest_seq(), Some(6));
        assert_eq!(replay.newest_seq(), Some(total));
    }

    #[test]
    fn replay_buffer_prunes_entries_outside_retention_window() {
        let now = neurohid_types::now_micros();
        let mut replay = RuntimeEventsReplayBuffer::default();
        replay.push(replay_item(1, now - RUNTIME_EVENTS_REPLAY_RETENTION_US - 1));
        replay.push(replay_item(2, now));

        assert_eq!(replay.oldest_seq(), Some(2));
        assert_eq!(replay.newest_seq(), Some(2));
    }

    fn allocate_test_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("ephemeral bind should succeed")
            .local_addr()
            .expect("socket address should resolve")
            .port()
    }

    async fn wait_for_runtime_start(runtime: &RuntimeHandle) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            if runtime.snapshot().running {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "runtime did not become active in time"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    fn test_control_snapshot() -> ControlSnapshot {
        ControlSnapshot {
            running: false,
            uptime_secs: 0,
            calibration_mode: false,
            output_enabled: true,
            profile_ready: false,
            decoder_ready: false,
            decoder_model_version: None,
            active_profile_name: None,
            device_name: None,
            outlet_name: None,
            signal_name: None,
            decoder_name: None,
            device_battery: None,
            signal_quality: 0.0,
            signal_latency_last_us: 0,
            signal_latency_p95_us: 0,
            decode_latency_last_us: 0,
            decode_latency_p95_us: 0,
            action_latency_last_us: 0,
            action_latency_p95_us: 0,
            latency_degraded: false,
            latency_alert_message: None,
            actions_emitted: 0,
            errors_detected: 0,
            ipc_connected: false,
            ipc_simulated: false,
            learning_enabled: true,
            ml_bridge_connected: false,
            ml_bridge_stalled: false,
            runtime_mode_state: RuntimeModeState::Degraded,
            enabled_capabilities: Vec::new(),
            limited_capabilities_message: None,
            fallback_model_kind: None,
            trainer_replay_size: None,
            trainer_step: None,
            trainer_policy_loss: None,
            trainer_value_loss: None,
            trainer_entropy: None,
            trainer_last_error: None,
            candidate_promotions_succeeded: 0,
            candidate_promotions_rejected: 0,
            candidate_last_outcome: None,
            ml_protocol_version: None,
            device_connected: false,
            task_error: None,
            discovered_streams: vec![],
            routed_eeg_streams: 0,
            routed_motion_streams: 0,
            routed_auxiliary_streams: 0,
            routed_unknown_streams: 0,
            pipeline_integrity_degraded: false,
            integrity_issue_count: 0,
            stage_health_summary: None,
            recording_active: false,
            current_session_id: None,
        }
    }

    #[tokio::test]
    async fn runtime_events_subscription_does_not_block_control_rpc() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
        config.service.ipc_simulation_enabled = true;
        config.action.enabled = false;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");
        wait_for_runtime_start(&runtime).await;

        let server_task = tokio::spawn(run_ipc_control_server(
            server_config.clone(),
            runtime.ipc_handle(),
            EmitPolicyConfig::default(),
        ));

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut events_client = RuntimeIpcClient::new(server_config.clone());
        events_client
            .connect()
            .await
            .expect("events client should connect");
        let subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("test-events".to_string()),
            &serde_json::json!({
                "families": ["snapshot"],
                "include_snapshot": true,
                "include_capabilities": false,
                "snapshot_interval_ms": 250,
                "max_duration_ms": 2_000
            }),
        )
        .expect("subscribe envelope should encode");
        events_client
            .send(subscribe)
            .await
            .expect("events subscribe should send");

        let first_event = tokio::time::timeout(Duration::from_secs(1), events_client.recv())
            .await
            .expect("events stream should produce a message")
            .expect("events receive should succeed");
        assert_eq!(first_event.channel, IpcChannel::RuntimeEvents);

        let mut control_client = RuntimeIpcClient::new(server_config);
        control_client
            .connect()
            .await
            .expect("control client should connect");
        let started = Instant::now();
        let response = tokio::time::timeout(
            Duration::from_millis(700),
            control_client.send_control_request(
                ControlRequest::new(ControlCommand::Snapshot),
                "test-control",
                1,
            ),
        )
        .await
        .expect("control request timed out")
        .expect("control request should succeed");
        assert!(
            started.elapsed() < Duration::from_millis(700),
            "control request was blocked by runtime.events stream"
        );
        assert!(matches!(
            response.payload,
            ControlResponsePayload::Snapshot { .. }
        ));

        let _ = control_client.disconnect().await;
        let _ = events_client.disconnect().await;
        server_task.abort();
        let _ = server_task.await;

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }

    #[tokio::test]
    async fn trainer_stream_rejects_second_active_session() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
        config.service.ipc_simulation_enabled = false;
        config.action.enabled = false;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");
        wait_for_runtime_start(&runtime).await;

        let server_task = tokio::spawn(run_ipc_control_server(
            server_config.clone(),
            runtime.ipc_handle(),
            EmitPolicyConfig::default(),
        ));
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut first_client = RuntimeIpcClient::new(server_config.clone());
        first_client
            .connect()
            .await
            .expect("first trainer client should connect");
        let hello = IpcEnvelope::new(
            IpcChannel::TrainerStream,
            "hello",
            1,
            None,
            Some("trainer-a".to_string()),
            &serde_json::json!({
                "protocol": "neurohid_runtime_ml_v3",
                "role": "trainer",
                "capabilities": [],
                "profile_id": null
            }),
        )
        .expect("trainer hello envelope should encode");
        first_client
            .send(hello)
            .await
            .expect("first trainer hello should send");
        let first_response = tokio::time::timeout(Duration::from_secs(1), first_client.recv())
            .await
            .expect("first trainer should receive bootstrap response")
            .expect("first trainer receive should succeed");
        assert_eq!(first_response.channel, IpcChannel::TrainerStream);

        let mut second_client = RuntimeIpcClient::new(server_config);
        second_client
            .connect()
            .await
            .expect("second trainer client should connect");
        let second_hello = IpcEnvelope::new(
            IpcChannel::TrainerStream,
            "hello",
            1,
            None,
            Some("trainer-b".to_string()),
            &serde_json::json!({
                "protocol": "neurohid_runtime_ml_v3",
                "role": "trainer",
                "capabilities": [],
                "profile_id": null
            }),
        )
        .expect("second trainer hello envelope should encode");
        second_client
            .send(second_hello)
            .await
            .expect("second trainer hello should send");
        let busy = second_client
            .recv()
            .await
            .expect("second trainer should receive busy error");
        assert_eq!(busy.channel, IpcChannel::TrainerStream);
        assert_eq!(busy.msg_type, "error");
        assert_eq!(
            busy.payload.get("code").and_then(serde_json::Value::as_str),
            Some("trainer_busy")
        );

        let _ = first_client.disconnect().await;
        let _ = second_client.disconnect().await;
        server_task.abort();
        let _ = server_task.await;

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }

    #[tokio::test]
    async fn runtime_events_resume_replay_hit_emits_lifecycle_metadata() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
        config.service.ipc_simulation_enabled = true;
        config.action.enabled = false;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");
        wait_for_runtime_start(&runtime).await;

        let server_task = tokio::spawn(run_ipc_control_server(
            server_config.clone(),
            runtime.ipc_handle(),
            EmitPolicyConfig::default(),
        ));
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut seed_client = RuntimeIpcClient::new(server_config.clone());
        seed_client
            .connect()
            .await
            .expect("seed client should connect");
        let seed_subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("seed-subscriber".to_string()),
            &serde_json::json!({
                "families": ["snapshot"],
                "include_snapshot": true,
                "include_capabilities": false,
                "max_events": 1,
                "max_duration_ms": 500,
            }),
        )
        .expect("seed subscribe envelope should encode");
        seed_client
            .send(seed_subscribe)
            .await
            .expect("seed subscribe should send");
        let first_event = seed_client
            .recv()
            .await
            .expect("seed subscriber should receive one event");
        let resume_from_seq = first_event.seq;
        let _ = seed_client.disconnect().await;

        let mut resume_client = RuntimeIpcClient::new(server_config.clone());
        resume_client
            .connect()
            .await
            .expect("resume client should connect");
        let resume_subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("resume-subscriber".to_string()),
            &serde_json::json!({
                "include_snapshot": false,
                "include_capabilities": false,
                "resume_from_seq": resume_from_seq,
                "max_events": 6,
                "max_duration_ms": 1_500,
            }),
        )
        .expect("resume subscribe envelope should encode");
        resume_client
            .send(resume_subscribe)
            .await
            .expect("resume subscribe should send");

        let mut saw_replay_resumed = false;
        for _ in 0..6 {
            let envelope = tokio::time::timeout(Duration::from_millis(500), resume_client.recv())
                .await
                .expect("resume subscriber should receive events")
                .expect("resume subscriber recv should succeed");
            if envelope.channel != IpcChannel::RuntimeEvents || envelope.msg_type != "event" {
                continue;
            }
            if envelope
                .payload
                .get("type")
                .and_then(serde_json::Value::as_str)
                == Some("lifecycle")
                && envelope
                    .payload
                    .get("state")
                    .and_then(serde_json::Value::as_str)
                    == Some("replay_resumed")
            {
                saw_replay_resumed = true;
                assert_eq!(
                    envelope
                        .payload
                        .get("requested_seq")
                        .and_then(serde_json::Value::as_u64),
                    Some(resume_from_seq)
                );
                assert!(
                    envelope
                        .payload
                        .get("replay_window_start_seq")
                        .and_then(serde_json::Value::as_u64)
                        .is_some()
                );
                assert!(
                    envelope
                        .payload
                        .get("replay_window_end_seq")
                        .and_then(serde_json::Value::as_u64)
                        .is_some()
                );
                break;
            }
        }

        assert!(
            saw_replay_resumed,
            "expected replay_resumed lifecycle event"
        );

        let _ = resume_client.disconnect().await;
        server_task.abort();
        let _ = server_task.await;

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }

    #[tokio::test]
    async fn runtime_events_resume_replay_miss_emits_lifecycle_metadata() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
        config.service.ipc_simulation_enabled = true;
        config.action.enabled = false;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");
        wait_for_runtime_start(&runtime).await;

        let server_task = tokio::spawn(run_ipc_control_server(
            server_config.clone(),
            runtime.ipc_handle(),
            EmitPolicyConfig::default(),
        ));
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = RuntimeIpcClient::new(server_config);
        client.connect().await.expect("client should connect");
        let subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("replay-miss-subscriber".to_string()),
            &serde_json::json!({
                "include_snapshot": false,
                "include_capabilities": false,
                "resume_from_seq": 0,
                "max_events": 3,
                "max_duration_ms": 1_000,
            }),
        )
        .expect("replay-miss subscribe envelope should encode");
        client
            .send(subscribe)
            .await
            .expect("replay-miss subscribe should send");

        let envelope = tokio::time::timeout(Duration::from_secs(1), client.recv())
            .await
            .expect("replay-miss subscriber should receive lifecycle")
            .expect("replay-miss recv should succeed");
        assert_eq!(envelope.channel, IpcChannel::RuntimeEvents);
        assert_eq!(envelope.msg_type, "event");
        assert_eq!(
            envelope
                .payload
                .get("type")
                .and_then(serde_json::Value::as_str),
            Some("lifecycle")
        );
        assert_eq!(
            envelope
                .payload
                .get("state")
                .and_then(serde_json::Value::as_str),
            Some("replay_miss")
        );
        assert_eq!(
            envelope
                .payload
                .get("requested_seq")
                .and_then(serde_json::Value::as_u64),
            Some(0)
        );

        let _ = client.disconnect().await;
        server_task.abort();
        let _ = server_task.await;

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }

    #[tokio::test]
    async fn runtime_events_emit_backpressure_drop_when_broker_subscriber_lags() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
        config.action.enabled = false;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");
        wait_for_runtime_start(&runtime).await;

        let server = RuntimeIpcServer::new(server_config.clone())
            .await
            .expect("ipc server should start");
        let mut client = RuntimeIpcClient::new(server_config);
        client.connect().await.expect("client should connect");
        let connection = server.accept().await.expect("server should accept client");

        let mut broker_config = RuntimeBrokerConfig::default();
        broker_config.runtime_events.capacity = 1;
        let broker = RuntimeIpcBroker::new(broker_config);
        let runtime_events_state = Arc::new(Mutex::new(RuntimeEventsState::default()));

        let subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("lagged-broker-subscriber".to_string()),
            &serde_json::json!({
                "families": ["backpressure_drop"],
                "include_snapshot": false,
                "include_capabilities": false,
                "max_events": 128,
                "max_duration_ms": 1_000,
            }),
        )
        .expect("subscribe envelope should encode");

        let reader_task = tokio::spawn(async move {
            let mut saw_drop = false;
            for _ in 0..128 {
                let envelope =
                    match tokio::time::timeout(Duration::from_millis(500), client.recv()).await {
                        Ok(Ok(envelope)) => envelope,
                        _ => break,
                    };
                if envelope.channel != IpcChannel::RuntimeEvents || envelope.msg_type != "event" {
                    continue;
                }

                if envelope
                    .payload
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    == Some("backpressure_drop")
                {
                    let dropped = envelope
                        .payload
                        .get("dropped")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or_default();
                    let reason = envelope
                        .payload
                        .get("reason")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    if dropped > 0 && reason.contains("broker subscriber lagged") {
                        saw_drop = true;
                        break;
                    }
                }

                if envelope
                    .payload
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    == Some("lifecycle")
                    && envelope
                        .payload
                        .get("state")
                        .and_then(serde_json::Value::as_str)
                        == Some("subscription_closed")
                {
                    break;
                }
            }

            let _ = client.disconnect().await;
            saw_drop
        });

        let publish_task = {
            let broker = broker.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(25)).await;
                for seq in 0..5_000_u64 {
                    let _ = broker.publish_runtime_event(RuntimeEvent::Lifecycle {
                        state: "burst".to_string(),
                        detail: seq.to_string(),
                        requested_seq: None,
                        replay_window_start_seq: None,
                        replay_window_end_seq: None,
                    });
                }
            })
        };

        let runtime_ipc = runtime.ipc_handle();
        handle_runtime_events_subscription(
            &connection,
            &runtime_ipc,
            &broker,
            subscribe,
            runtime_events_state,
        )
        .await
        .expect("runtime.events subscription handler should complete");

        publish_task.await.expect("publisher task should complete");
        let saw_drop = reader_task.await.expect("reader task should complete");
        assert!(
            saw_drop,
            "expected runtime.events backpressure_drop event from broker lag"
        );
        let counters = broker.counters();
        assert!(
            counters.runtime_backpressure_drops > 0,
            "expected runtime backpressure drop counter to increase"
        );
        assert!(
            counters.subscriber_lag_events > 0,
            "expected subscriber lag events counter to increase"
        );

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }

    #[tokio::test]
    async fn runtime_events_replay_updates_broker_replay_counters() {
        let port = allocate_test_port();
        let server_config = RuntimeIpcConfig {
            transport: RuntimeIpcTransport::TcpLoopback,
            endpoint: format!("127.0.0.1:{port}"),
            ..RuntimeIpcConfig::default()
        };

        let mut config = SystemConfig::default();
        config.device.backend = DeviceBackend::BrainFlow;
        config.device.brainflow = Some(BrainFlowConfig::default());
        config.action.enabled = false;

        let runtime = RuntimeBuilder::new(config)
            .start()
            .await
            .expect("runtime should start");
        wait_for_runtime_start(&runtime).await;

        let server = RuntimeIpcServer::new(server_config.clone())
            .await
            .expect("ipc server should start");
        let broker = RuntimeIpcBroker::new(RuntimeBrokerConfig::default());
        let runtime_events_state = Arc::new(Mutex::new(RuntimeEventsState::default()));

        {
            let mut state = runtime_events_state.lock().await;
            let now = neurohid_types::now_micros();
            for offset in 0..3_u64 {
                let seq = state.allocate_seq();
                state.replay.push(RuntimeEventsReplayItem {
                    seq,
                    sent_at_us: now.saturating_add(offset as i64),
                    family: "lifecycle",
                    event: RuntimeEvent::Lifecycle {
                        state: "seed".to_string(),
                        detail: format!("seed-{seq}"),
                        requested_seq: None,
                        replay_window_start_seq: None,
                        replay_window_end_seq: None,
                    },
                });
            }
        }

        let mut hit_client = RuntimeIpcClient::new(server_config.clone());
        hit_client
            .connect()
            .await
            .expect("hit client should connect");
        let hit_connection = server
            .accept()
            .await
            .expect("server should accept hit client");
        let hit_subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("replay-hit-counters".to_string()),
            &serde_json::json!({
                "include_snapshot": false,
                "include_capabilities": false,
                "resume_from_seq": 2,
                "max_events": 4,
                "max_duration_ms": 500,
            }),
        )
        .expect("hit subscribe envelope should encode");
        let hit_reader = tokio::spawn(async move {
            for _ in 0..8 {
                let envelope =
                    match tokio::time::timeout(Duration::from_millis(400), hit_client.recv()).await
                    {
                        Ok(Ok(envelope)) => envelope,
                        _ => break,
                    };
                if envelope
                    .payload
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    == Some("lifecycle")
                    && envelope
                        .payload
                        .get("state")
                        .and_then(serde_json::Value::as_str)
                        == Some("subscription_closed")
                {
                    break;
                }
            }
            let _ = hit_client.disconnect().await;
        });

        let runtime_ipc = runtime.ipc_handle();
        handle_runtime_events_subscription(
            &hit_connection,
            &runtime_ipc,
            &broker,
            hit_subscribe,
            Arc::clone(&runtime_events_state),
        )
        .await
        .expect("replay-hit subscription should complete");
        hit_reader.await.expect("hit reader task should complete");

        let counters_after_hit = broker.counters();
        assert_eq!(counters_after_hit.replay_hits, 1);
        assert_eq!(counters_after_hit.replay_misses, 0);

        let mut miss_client = RuntimeIpcClient::new(server_config);
        miss_client
            .connect()
            .await
            .expect("miss client should connect");
        let miss_connection = server
            .accept()
            .await
            .expect("server should accept miss client");
        let miss_subscribe = IpcEnvelope::new(
            IpcChannel::RuntimeEvents,
            "subscribe",
            1,
            None,
            Some("replay-miss-counters".to_string()),
            &serde_json::json!({
                "include_snapshot": false,
                "include_capabilities": false,
                "resume_from_seq": 0,
                "max_events": 2,
                "max_duration_ms": 500,
            }),
        )
        .expect("miss subscribe envelope should encode");
        let miss_reader =
            tokio::spawn(async move {
                for _ in 0..6 {
                    let envelope =
                        match tokio::time::timeout(Duration::from_millis(400), miss_client.recv())
                            .await
                        {
                            Ok(Ok(envelope)) => envelope,
                            _ => break,
                        };
                    if envelope
                        .payload
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        == Some("lifecycle")
                        && envelope
                            .payload
                            .get("state")
                            .and_then(serde_json::Value::as_str)
                            == Some("subscription_closed")
                    {
                        break;
                    }
                }
                let _ = miss_client.disconnect().await;
            });

        handle_runtime_events_subscription(
            &miss_connection,
            &runtime_ipc,
            &broker,
            miss_subscribe,
            runtime_events_state,
        )
        .await
        .expect("replay-miss subscription should complete");
        miss_reader.await.expect("miss reader task should complete");

        let counters_after_miss = broker.counters();
        assert_eq!(counters_after_miss.replay_hits, 1);
        assert_eq!(counters_after_miss.replay_misses, 1);

        runtime
            .command(RuntimeCommand::Stop)
            .expect("stop command should succeed");
        runtime.wait().await.expect("runtime should stop cleanly");
    }

    #[test]
    fn capabilities_mark_live_components_unavailable_when_runtime_not_ready() {
        let mut snapshot = test_control_snapshot();
        snapshot.running = false;
        snapshot.device_connected = false;
        snapshot.profile_ready = false;
        snapshot.decoder_ready = false;
        snapshot.ml_bridge_connected = false;
        snapshot.ml_bridge_stalled = false;

        let RuntimeEvent::Capabilities { components, .. } =
            build_runtime_capabilities_event(&snapshot)
        else {
            panic!("expected capabilities event");
        };

        let sample = components
            .iter()
            .find(|component| component.name == "sample")
            .expect("sample capability should exist");
        assert!(!sample.available);
        assert_eq!(
            sample.unavailable_reason.as_deref(),
            Some("runtime_not_running")
        );

        let decision = components
            .iter()
            .find(|component| component.name == "decision_event")
            .expect("decision_event capability should exist");
        assert!(!decision.available);
    }

    #[test]
    fn capabilities_mark_trainer_components_available_when_runtime_ready() {
        let mut snapshot = test_control_snapshot();
        snapshot.running = true;
        snapshot.device_connected = true;
        snapshot.profile_ready = true;
        snapshot.decoder_ready = true;
        snapshot.ml_bridge_connected = true;
        snapshot.ml_bridge_stalled = false;

        let RuntimeEvent::Capabilities { components, .. } =
            build_runtime_capabilities_event(&snapshot)
        else {
            panic!("expected capabilities event");
        };

        for capability in ["sample", "feature_frame", "decision_event", "errp_result"] {
            let component = components
                .iter()
                .find(|component| component.name == capability)
                .unwrap_or_else(|| panic!("{capability} capability should exist"));
            assert!(component.available, "{capability} should be available");
            assert!(component.unavailable_reason.is_none());
        }
    }
}
