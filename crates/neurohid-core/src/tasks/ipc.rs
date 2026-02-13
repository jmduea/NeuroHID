//! Runtime ML bridge task (protocol v2).

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use neurohid_storage::ProfileStore;
use tokio::sync::{broadcast, mpsc, RwLock};

use neurohid_ipc::{
    AckV2, CandidateModelReadyV2, DecisionEventV2, ErrpResultV2, HelloV2, IpcConfig, IpcServer,
    IpcTransport, ProtocolErrorV2, RuntimeMlEnvelopeV2, RuntimeMlKindV2, RuntimeMlRoleV2,
    RuntimeTelemetryV2, SessionBoundaryEventV2, SessionBoundaryV2, ShutdownV2, TrainerStatusV2,
    RUNTIME_ML_PROTOCOL_V2,
};
use neurohid_types::{
    control::RuntimeModeState,
    error::{Error, IpcError, Result},
    event::{MarkerPayload, MarkerType, StreamMarker},
    profile::ProfileId,
    reward::{ErrPResult, SignalQuality},
};

use crate::service::{DecoderCommand, ServiceState};
use crate::tasks::DecisionEventRecord;

const REAL_MESSAGE_POLL_MS: u64 = 25;
const SIMULATED_CONNECT_DELAY_MS: u64 = 100;

/// Runtime ML bridge task.
pub struct IpcTask {
    config: neurohid_types::config::ServiceConfig,
    decision_rx: mpsc::Receiver<DecisionEventRecord>,
    errp_tx: mpsc::Sender<ErrPResult>,
    state: Arc<RwLock<ServiceState>>,
    marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,
    profile_store: Option<ProfileStore>,
    decoder_command_tx: Option<mpsc::Sender<DecoderCommand>>,
    send_sequence: u64,
    session_id: String,
    dropped_messages: u64,
}

impl IpcTask {
    pub fn new(
        config: neurohid_types::config::ServiceConfig,
        decision_rx: mpsc::Receiver<DecisionEventRecord>,
        errp_tx: mpsc::Sender<ErrPResult>,
        state: Arc<RwLock<ServiceState>>,
        marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,
        profile_store: Option<ProfileStore>,
        decoder_command_tx: Option<mpsc::Sender<DecoderCommand>>,
    ) -> Self {
        Self {
            config,
            decision_rx,
            errp_tx,
            state,
            marker_broadcast_tx,
            profile_store,
            decoder_command_tx,
            send_sequence: 0,
            session_id: neurohid_types::now_micros().to_string(),
            dropped_messages: 0,
        }
    }

    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        tracing::info!("IPC task started");

        let result = if self.config.ipc_simulation_enabled {
            self.run_simulated(&mut shutdown).await
        } else {
            self.run_real_bridge(&mut shutdown).await
        };

        self.set_connection_state(false, false, false).await;
        tracing::info!("IPC task completed");
        result
    }

    async fn run_real_bridge(&mut self, shutdown: &mut broadcast::Receiver<()>) -> Result<()> {
        let transport = match self.config.ml_transport {
            neurohid_types::config::MlTransport::NamedPipe => IpcTransport::NamedPipe,
            neurohid_types::config::MlTransport::TcpLoopback => IpcTransport::TcpLoopback,
        };
        let server = IpcServer::new(IpcConfig {
            transport,
            address: format!("127.0.0.1:{}", self.config.ipc_port),
            pipe_name: self.config.ml_pipe_name.clone(),
            ..IpcConfig::default()
        })
        .await?;

        loop {
            tracing::info!(endpoint = %server.endpoint(), "Waiting for trainer bridge to connect");

            let connection = tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("IPC task received shutdown signal");
                    return Ok(());
                }
                result = server.accept() => result?,
            };

            self.set_connection_state(true, false, false).await;
            self.note_heartbeat().await;
            self.send_hello(&connection).await?;
            self.send_session_boundary_start(&connection).await?;
            tracing::info!("Trainer bridge connected");

            let run_result = self.run_connected_loop(connection, shutdown).await;
            self.set_connection_state(false, false, false).await;

            match run_result {
                Ok(()) => return Ok(()),
                Err(err) if is_connection_lost_error(&err) => {
                    tracing::warn!("Trainer bridge disconnected; waiting for reconnect");
                }
                Err(err) => return Err(err),
            }
        }
    }

    async fn run_connected_loop(
        &mut self,
        connection: neurohid_ipc::server::IpcConnection,
        shutdown: &mut broadcast::Receiver<()>,
    ) -> Result<()> {
        let mut poll = tokio::time::interval(Duration::from_millis(REAL_MESSAGE_POLL_MS));
        let mut heartbeat_tick =
            tokio::time::interval(Duration::from_millis(self.config.ml_heartbeat_interval_ms));
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        heartbeat_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_msg_at = Instant::now();

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("IPC task received shutdown signal");
                    let shutdown_payload = ShutdownV2 { reason: "runtime_shutdown".to_string() };
                    let msg = self.build_envelope(RuntimeMlKindV2::Shutdown, &shutdown_payload)?;
                    let _ = connection.send(msg).await;
                    break;
                }
                _ = poll.tick() => {
                    self.drain_trainer_messages(&connection).await?;
                    let since_last = last_msg_at.elapsed().as_millis() as u64;
                    if since_last > self.config.ml_stall_timeout_ms {
                        self.set_stalled(true).await;
                    }
                    self.publish_runtime_telemetry(&connection).await?;
                }
                _ = heartbeat_tick.tick() => {
                    let ping = neurohid_ipc::PingV2 {
                        ping_id: format!("ping_{}", self.send_sequence.saturating_add(1)),
                        timestamp_us: neurohid_types::now_micros(),
                    };
                    let msg = self.build_envelope(RuntimeMlKindV2::Ping, &ping)?;
                    connection.send(msg).await?;
                }
                decision = self.decision_rx.recv() => {
                    let Some(decision) = decision else {
                        tracing::info!("Decision event channel closed");
                        break;
                    };
                    self.send_decision_event_real(&connection, decision).await?;
                }
                recv = connection.recv() => {
                    let message = recv?;
                    last_msg_at = Instant::now();
                    self.handle_trainer_message(message).await?;
                }
            }
        }

        Ok(())
    }

    async fn run_simulated(&mut self, shutdown: &mut broadcast::Receiver<()>) -> Result<()> {
        tracing::info!("IPC simulation mode enabled; starting simulated bridge");
        tokio::time::sleep(Duration::from_millis(SIMULATED_CONNECT_DELAY_MS)).await;
        self.set_connection_state(true, true, false).await;
        self.note_heartbeat().await;
        tracing::info!("Trainer bridge connected (simulated)");

        loop {
            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!("IPC task received shutdown signal");
                    break;
                }
                decision = self.decision_rx.recv() => {
                    let Some(decision) = decision else {
                        tracing::info!("Decision event channel closed");
                        break;
                    };
                    // In simulation mode, emit neutral ErrP feedback so downstream
                    // rolling success metrics remain stable.
                    let result = ErrPResult {
                        action_timestamp: decision.timestamp_us,
                        detection_timestamp: neurohid_types::now_micros(),
                        error_probability: 0.0,
                        classification_confidence: 0.0,
                        signal_quality: SignalQuality::Good,
                        estimated_magnitude: None,
                        detection_latency_us: 0,
                    };
                    let _ = self.errp_tx.send(result).await;
                }
            }
        }

        Ok(())
    }

    async fn drain_trainer_messages(
        &mut self,
        connection: &neurohid_ipc::server::IpcConnection,
    ) -> Result<()> {
        loop {
            match connection.try_recv()? {
                Some(msg) => self.handle_trainer_message(msg).await?,
                None => break,
            }
        }
        Ok(())
    }

    async fn send_hello(&mut self, connection: &neurohid_ipc::server::IpcConnection) -> Result<()> {
        let hello = HelloV2 {
            protocol: "neurohid_runtime_ml_v2".to_string(),
            role: RuntimeMlRoleV2::Runtime,
            capabilities: vec![
                "errp_window_stream".to_string(),
                "candidate_guarded_promotion".to_string(),
            ],
            profile_id: self.state.read().await.active_profile_name.clone(),
            feature_schema_version: Some(neurohid_types::model::CURRENT_FEATURE_SCHEMA_VERSION),
            action_schema_version: Some(neurohid_types::model::CURRENT_ACTION_SCHEMA_VERSION),
            decoder_model_version: self.state.read().await.decoder_model_version.clone(),
            trainer_name: None,
            trainer_version: None,
        };
        let msg = self.build_envelope(RuntimeMlKindV2::Hello, &hello)?;
        connection.send(msg).await
    }

    async fn send_session_boundary_start(
        &mut self,
        connection: &neurohid_ipc::server::IpcConnection,
    ) -> Result<()> {
        let boundary = SessionBoundaryV2 {
            event: SessionBoundaryEventV2::Start,
            reason: "runtime_boot".to_string(),
            started_at_us: neurohid_types::now_micros(),
        };
        let msg = self.build_envelope(RuntimeMlKindV2::SessionBoundary, &boundary)?;
        connection.send(msg).await
    }

    async fn send_decision_event_real(
        &mut self,
        connection: &neurohid_ipc::server::IpcConnection,
        decision: DecisionEventRecord,
    ) -> Result<()> {
        if let Some(tx) = &self.marker_broadcast_tx {
            let marker = StreamMarker::now(MarkerType::ErrpWindowStart).with_payload(
                MarkerPayload::ErrpWindow {
                    sequence: self.send_sequence.saturating_add(1),
                    action_timestamp: decision.timestamp_us,
                },
            );
            let _ = tx.send(marker);
        }

        let payload = DecisionEventV2 {
            decision_id: decision.decision_id,
            timestamp_us: decision.timestamp_us,
            feature_values: decision.feature_values,
            action: decision.action,
            decoder_confidence: decision.decoder_confidence,
            signal_quality: decision.signal_quality,
            decoder_model_version: decision.decoder_model_version,
            stream_id: decision.stream_id,
        };
        let msg = self.build_envelope(RuntimeMlKindV2::DecisionEvent, &payload)?;
        connection.send(msg).await
    }

    async fn publish_runtime_telemetry(
        &mut self,
        connection: &neurohid_ipc::server::IpcConnection,
    ) -> Result<()> {
        let state = self.state.read().await;
        let telemetry = RuntimeTelemetryV2 {
            signal_latency_p95_us: state.signal_latency_p95_us,
            decode_latency_p95_us: state.decode_latency_p95_us,
            action_latency_p95_us: state.action_latency_p95_us,
            decision_queue_depth: 0,
            errp_queue_depth: 0,
            dropped_ml_messages: self.dropped_messages,
        };
        drop(state);
        let msg = self.build_envelope(RuntimeMlKindV2::RuntimeTelemetry, &telemetry)?;
        connection.send(msg).await
    }

    async fn handle_trainer_message(&mut self, message: RuntimeMlEnvelopeV2) -> Result<()> {
        self.note_heartbeat().await;
        self.set_stalled(false).await;

        if message.v != RUNTIME_ML_PROTOCOL_V2 {
            tracing::warn!(
                version = message.v,
                "Received incompatible ML protocol version (expected v2)"
            );
            return Ok(());
        }

        match message.kind {
            RuntimeMlKindV2::Hello => {
                let hello: HelloV2 = message.decode_payload().map_err(IpcError::InvalidMessage)?;
                let mut state = self.state.write().await;
                state.ml_protocol_version = Some(message.v);
                state.ml_bridge_connected = true;
                if hello.role != RuntimeMlRoleV2::Trainer {
                    tracing::warn!("ML hello role is not trainer");
                }
            }
            RuntimeMlKindV2::ErrpResult => {
                let payload: ErrpResultV2 =
                    message.decode_payload().map_err(IpcError::InvalidMessage)?;
                if let Some(tx) = &self.marker_broadcast_tx {
                    let marker = StreamMarker::now(MarkerType::ErrpWindowResult).with_payload(
                        MarkerPayload::ErrpResult {
                            sequence: message.seq,
                            error_probability: payload.error_probability,
                        },
                    );
                    let _ = tx.send(marker);
                }

                let result = ErrPResult {
                    action_timestamp: payload.action_timestamp_us,
                    detection_timestamp: payload.detection_timestamp_us,
                    error_probability: payload.error_probability,
                    classification_confidence: payload.classification_confidence,
                    signal_quality: match payload.signal_quality.as_str() {
                        "good" => SignalQuality::Good,
                        "acceptable" => SignalQuality::Acceptable,
                        "poor" => SignalQuality::Poor,
                        _ => SignalQuality::Unusable,
                    },
                    estimated_magnitude: payload.estimated_magnitude,
                    detection_latency_us: payload.detection_latency_us,
                };
                if self.errp_tx.send(result).await.is_err() {
                    tracing::warn!("ErrP receiver dropped");
                }
            }
            RuntimeMlKindV2::TrainerStatus => {
                let status: TrainerStatusV2 =
                    message.decode_payload().map_err(IpcError::InvalidMessage)?;
                let mut state = self.state.write().await;
                state.trainer_replay_size = Some(status.replay_size);
                state.trainer_step = Some(status.training_step);
            }
            RuntimeMlKindV2::CandidateModelReady => {
                let candidate: CandidateModelReadyV2 =
                    message.decode_payload().map_err(IpcError::InvalidMessage)?;
                if !self.state.read().await.learning_enabled {
                    tracing::info!(
                        profile = %candidate.profile_id,
                        run_id = %candidate.source_run_id,
                        "Ignoring candidate model because learning is disabled"
                    );
                    return Ok(());
                }

                if let Some(store) = &self.profile_store {
                    let profile_id = ProfileId::new(&candidate.profile_id);
                    if let Err(error) = store
                        .import_decoder_candidate_from_dir(
                            &profile_id,
                            Path::new(&candidate.artifact_dir),
                        )
                        .await
                    {
                        tracing::warn!(
                            profile = %candidate.profile_id,
                            artifact_dir = %candidate.artifact_dir,
                            "Failed to import candidate model artifacts: {}",
                            error
                        );
                        return Ok(());
                    }
                }

                if let Some(tx) = &self.decoder_command_tx {
                    if tx
                        .send(DecoderCommand::PromoteCandidateModel)
                        .await
                        .is_err()
                    {
                        tracing::warn!(
                            "Decoder command channel dropped before candidate promotion"
                        );
                        return Ok(());
                    }
                } else {
                    tracing::warn!("No decoder command channel available for candidate promotion");
                }

                tracing::info!(
                    profile = %candidate.profile_id,
                    run_id = %candidate.source_run_id,
                    "Candidate model ready notification processed"
                );
            }
            RuntimeMlKindV2::Pong => {
                tracing::trace!("Received trainer pong");
            }
            RuntimeMlKindV2::Ack => {
                let _: AckV2 = message.decode_payload().map_err(IpcError::InvalidMessage)?;
            }
            RuntimeMlKindV2::Error => {
                let err: ProtocolErrorV2 =
                    message.decode_payload().map_err(IpcError::InvalidMessage)?;
                tracing::warn!(recoverable = err.recoverable, code = %err.code, "Trainer error: {}", err.message);
                if !err.recoverable {
                    return Err(IpcError::ReceiveFailed(err.message).into());
                }
            }
            _ => {
                tracing::trace!(kind = ?message.kind, "Ignoring unsupported trainer message kind");
            }
        }

        Ok(())
    }

    fn build_envelope<T: serde::Serialize>(
        &mut self,
        kind: RuntimeMlKindV2,
        payload: &T,
    ) -> Result<RuntimeMlEnvelopeV2> {
        self.send_sequence = self.send_sequence.saturating_add(1);
        RuntimeMlEnvelopeV2::new(kind, self.send_sequence, self.session_id.clone(), payload)
            .map_err(|e| IpcError::InvalidMessage(e).into())
    }

    async fn set_connection_state(&self, connected: bool, simulated: bool, stalled: bool) {
        let mut state = self.state.write().await;
        state.ipc_connected = connected;
        state.ipc_simulated = simulated;
        state.ml_bridge_connected = connected;
        state.ml_bridge_stalled = stalled;
        if !connected {
            state.runtime_mode_state = RuntimeModeState::Fallback;
            state.limited_capabilities_message =
                Some("Trainer bridge disconnected; runtime fallback mode is active.".to_string());
        }
    }

    async fn set_stalled(&self, stalled: bool) {
        let mut state = self.state.write().await;
        state.ml_bridge_stalled = stalled;
        state.runtime_mode_state = if stalled {
            RuntimeModeState::Fallback
        } else if state.fallback_model_kind.as_deref() == Some("none") {
            RuntimeModeState::Degraded
        } else if state.fallback_model_kind.as_deref() == Some("lightweight_rust") {
            RuntimeModeState::Fallback
        } else if state.ml_bridge_connected {
            RuntimeModeState::Full
        } else {
            RuntimeModeState::Fallback
        };
        state.limited_capabilities_message = if stalled {
            Some("Trainer bridge heartbeat timed out; runtime fallback mode is active.".to_string())
        } else if matches!(state.runtime_mode_state, RuntimeModeState::Full) {
            None
        } else {
            state.limited_capabilities_message.clone()
        };
    }

    async fn note_heartbeat(&self) {
        let mut state = self.state.write().await;
        state.ml_bridge_last_heartbeat_us = Some(neurohid_types::now_micros());
    }
}

fn is_connection_lost_error(err: &Error) -> bool {
    matches!(err, Error::Ipc(IpcError::ConnectionLost))
}
