//! Runtime ML bridge task (protocol v2).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use neurohid_storage::ProfileStore;
use tokio::fs;
use tokio::sync::{broadcast, mpsc, RwLock};

use neurohid_ipc::{
    AckV2, CandidateModelReadyV2, DecisionEventV2, ErrpResultV2, ErrpWindowV2, HelloV2, IpcConfig,
    IpcServer, IpcTransport, ProtocolErrorV2, RuntimeMlEnvelopeV2, RuntimeMlKindV2,
    RuntimeMlRoleV2, RuntimeTelemetryV2, SessionBoundaryEventV2, SessionBoundaryV2, ShutdownV2,
    TrainerStatusV2, RUNTIME_ML_PROTOCOL_V2,
};
use neurohid_types::{
    control::RuntimeModeState,
    error::{Error, IpcError, Result},
    event::{MarkerPayload, MarkerType, StreamMarker},
    profile::ProfileId,
    reward::{ErrPConfig, ErrPResult, SignalQuality},
    signal::Sample,
};

use crate::service::{DecoderCommand, ServiceState};
use crate::tasks::DecisionEventRecord;

const REAL_MESSAGE_POLL_MS: u64 = 25;
const SIMULATED_CONNECT_DELAY_MS: u64 = 100;
const DEFAULT_ERRP_STREAM_KEY: &str = "__all__";
const ERRP_BUFFER_RETENTION_US: i64 = 5_000_000;
const ERRP_EMIT_GRACE_US: i64 = 120_000;
const DEFAULT_ERRP_SAMPLE_RATE_HZ: f32 = 128.0;
const MAX_CANDIDATE_FUTURE_SKEW_US: i64 = 5 * 60 * 1_000_000;
const MAX_CANDIDATE_MODEL_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone)]
struct PendingErrpWindow {
    decision_id: String,
    action_timestamp_us: i64,
    window_start_us: i64,
    window_end_us: i64,
    stream_id: Option<String>,
    signal_quality: f32,
}

#[derive(Debug, Clone)]
struct StreamSampleBuffer {
    samples: VecDeque<Sample>,
}

impl StreamSampleBuffer {
    fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(1024),
        }
    }

    fn push(&mut self, sample: Sample) {
        self.samples.push_back(sample);
    }

    fn prune_before(&mut self, cutoff_us: i64) {
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

/// Runtime ML bridge task.
pub struct IpcTask {
    config: neurohid_types::config::ServiceConfig,
    decision_rx: mpsc::Receiver<DecisionEventRecord>,
    errp_tx: mpsc::Sender<ErrPResult>,
    sample_rx: mpsc::Receiver<Sample>,
    errp_config: ErrPConfig,
    state: Arc<RwLock<ServiceState>>,
    marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,
    profile_store: Option<ProfileStore>,
    decoder_command_tx: Option<mpsc::Sender<DecoderCommand>>,
    send_sequence: u64,
    session_id: String,
    dropped_messages: u64,
    pending_errp_windows: VecDeque<PendingErrpWindow>,
    sample_buffers: HashMap<String, StreamSampleBuffer>,
}

impl IpcTask {
    pub fn new(
        config: neurohid_types::config::ServiceConfig,
        decision_rx: mpsc::Receiver<DecisionEventRecord>,
        errp_tx: mpsc::Sender<ErrPResult>,
        sample_rx: mpsc::Receiver<Sample>,
        errp_config: ErrPConfig,
        state: Arc<RwLock<ServiceState>>,
        marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,
        profile_store: Option<ProfileStore>,
        decoder_command_tx: Option<mpsc::Sender<DecoderCommand>>,
    ) -> Self {
        Self {
            config,
            decision_rx,
            errp_tx,
            sample_rx,
            errp_config,
            state,
            marker_broadcast_tx,
            profile_store,
            decoder_command_tx,
            send_sequence: 0,
            session_id: neurohid_types::now_micros().to_string(),
            dropped_messages: 0,
            pending_errp_windows: VecDeque::new(),
            sample_buffers: HashMap::new(),
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
            self.pending_errp_windows.clear();

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
                    self.emit_due_errp_windows(&connection, None).await?;
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
                sample = self.sample_rx.recv() => {
                    if let Some(sample) = sample {
                        let sample_timestamp = sample.device_timestamp.unwrap_or(sample.system_timestamp);
                        self.record_runtime_sample(sample);
                        self.emit_due_errp_windows(&connection, Some(sample_timestamp)).await?;
                    }
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

        let decision_id = decision.decision_id;
        let timestamp_us = decision.timestamp_us;
        let signal_quality = decision.signal_quality;
        let stream_id = decision.stream_id.clone();

        let payload = DecisionEventV2 {
            decision_id: decision_id.clone(),
            timestamp_us: decision.timestamp_us,
            feature_values: decision.feature_values,
            action: decision.action,
            decoder_confidence: decision.decoder_confidence,
            signal_quality: decision.signal_quality,
            decoder_model_version: decision.decoder_model_version,
            stream_id: decision.stream_id,
        };
        let msg = self.build_envelope(RuntimeMlKindV2::DecisionEvent, &payload)?;
        connection.send(msg).await?;
        self.queue_errp_window(decision_id, timestamp_us, stream_id, signal_quality);
        self.emit_due_errp_windows(connection, None).await
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
                state.trainer_policy_loss = status.policy_loss;
                state.trainer_value_loss = status.value_loss;
                state.trainer_entropy = status.entropy;
                state.trainer_last_error = status.last_error;
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

                let artifact_dir = match self.validate_candidate_notification(&candidate).await {
                    Ok(path) => path,
                    Err(error) => {
                        {
                            let mut state = self.state.write().await;
                            state.candidate_promotions_rejected =
                                state.candidate_promotions_rejected.saturating_add(1);
                            state.candidate_last_outcome =
                                Some(format!("candidate rejected: {}", error));
                        }
                        tracing::warn!(
                            profile = %candidate.profile_id,
                            artifact_dir = %candidate.artifact_dir,
                            run_id = %candidate.source_run_id,
                            "Rejected candidate model notification: {}",
                            error
                        );
                        return Ok(());
                    }
                };

                if let Some(store) = &self.profile_store {
                    let profile_id = ProfileId::new(&candidate.profile_id);
                    if let Err(error) = store
                        .import_decoder_candidate_from_dir(&profile_id, &artifact_dir)
                        .await
                    {
                        let mut state = self.state.write().await;
                        state.candidate_promotions_rejected =
                            state.candidate_promotions_rejected.saturating_add(1);
                        state.candidate_last_outcome = Some(format!(
                            "candidate import failed for profile {}",
                            candidate.profile_id
                        ));
                        tracing::warn!(
                            profile = %candidate.profile_id,
                            artifact_dir = %artifact_dir.display(),
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
                        let mut state = self.state.write().await;
                        state.candidate_promotions_rejected =
                            state.candidate_promotions_rejected.saturating_add(1);
                        state.candidate_last_outcome = Some(
                            "candidate promotion request failed: decoder command channel dropped"
                                .to_string(),
                        );
                        return Ok(());
                    }
                } else {
                    tracing::warn!("No decoder command channel available for candidate promotion");
                    let mut state = self.state.write().await;
                    state.candidate_promotions_rejected =
                        state.candidate_promotions_rejected.saturating_add(1);
                    state.candidate_last_outcome = Some(
                        "candidate promotion request failed: decoder command channel unavailable"
                            .to_string(),
                    );
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

    fn queue_errp_window(
        &mut self,
        decision_id: String,
        action_timestamp_us: i64,
        stream_id: Option<String>,
        signal_quality: f32,
    ) {
        let window_start_us = action_timestamp_us.saturating_add(self.errp_config.window_start_us);
        let mut window_end_us = action_timestamp_us.saturating_add(self.errp_config.window_end_us);
        if window_end_us <= window_start_us {
            window_end_us = window_start_us.saturating_add(1);
        }

        self.pending_errp_windows.push_back(PendingErrpWindow {
            decision_id,
            action_timestamp_us,
            window_start_us,
            window_end_us,
            stream_id,
            signal_quality,
        });
    }

    fn record_runtime_sample(&mut self, sample: Sample) {
        let sample_timestamp = sample.device_timestamp.unwrap_or(sample.system_timestamp);
        let cutoff_us = sample_timestamp.saturating_sub(ERRP_BUFFER_RETENTION_US);

        self.sample_buffers
            .entry(DEFAULT_ERRP_STREAM_KEY.to_string())
            .or_insert_with(StreamSampleBuffer::new)
            .push(sample.clone());

        if let Some(source_id) = sample.source_id.clone() {
            self.sample_buffers
                .entry(source_id)
                .or_insert_with(StreamSampleBuffer::new)
                .push(sample);
        }

        for buffer in self.sample_buffers.values_mut() {
            buffer.prune_before(cutoff_us);
            while buffer.samples.len() > 8_192 {
                let _ = buffer.samples.pop_front();
            }
        }
    }

    async fn emit_due_errp_windows(
        &mut self,
        connection: &neurohid_ipc::server::IpcConnection,
        latest_sample_timestamp: Option<i64>,
    ) -> Result<()> {
        let watermark = latest_sample_timestamp.unwrap_or_else(neurohid_types::now_micros);

        while let Some(next) = self.pending_errp_windows.front() {
            let now_us = neurohid_types::now_micros();
            let should_emit = watermark >= next.window_end_us
                || now_us >= next.window_end_us.saturating_add(ERRP_EMIT_GRACE_US);

            if !should_emit {
                break;
            }

            let pending = self.pending_errp_windows.pop_front().expect("front exists");
            let payload = self.build_errp_window_payload(&pending);
            let msg = self.build_envelope(RuntimeMlKindV2::ErrpWindow, &payload)?;
            connection.send(msg).await?;
        }

        Ok(())
    }

    fn build_errp_window_payload(&self, pending: &PendingErrpWindow) -> ErrpWindowV2 {
        let stream_key = pending
            .stream_id
            .as_deref()
            .unwrap_or(DEFAULT_ERRP_STREAM_KEY);
        let mut samples = self.collect_window_samples(stream_key, pending);
        if samples.is_empty() && stream_key != DEFAULT_ERRP_STREAM_KEY {
            samples = self.collect_window_samples(DEFAULT_ERRP_STREAM_KEY, pending);
        }

        let channel_count = samples
            .iter()
            .map(|sample| sample.values.len())
            .max()
            .unwrap_or(0);
        let mut channel_data = vec![Vec::with_capacity(samples.len()); channel_count];

        for sample in &samples {
            for (ch, values) in channel_data.iter_mut().enumerate() {
                values.push(sample.values.get(ch).copied().unwrap_or(0.0));
            }
        }

        let channel_labels = (0..channel_count)
            .map(|ch| format!("ch{}", ch + 1))
            .collect();

        ErrpWindowV2 {
            decision_id: pending.decision_id.clone(),
            action_timestamp_us: pending.action_timestamp_us,
            window_start_us: pending.window_start_us,
            window_end_us: pending.window_end_us,
            sample_rate_hz: Self::estimate_sample_rate_hz(&samples),
            channel_labels,
            channel_data,
            signal_quality: pending.signal_quality.clamp(0.0, 1.0),
        }
    }

    fn collect_window_samples(
        &self,
        stream_key: &str,
        pending: &PendingErrpWindow,
    ) -> Vec<&Sample> {
        let Some(buffer) = self.sample_buffers.get(stream_key) else {
            return Vec::new();
        };

        buffer
            .samples
            .iter()
            .filter(|sample| {
                let timestamp = sample.device_timestamp.unwrap_or(sample.system_timestamp);
                timestamp >= pending.window_start_us && timestamp <= pending.window_end_us
            })
            .collect()
    }

    fn estimate_sample_rate_hz(samples: &[&Sample]) -> f32 {
        if samples.len() < 2 {
            return DEFAULT_ERRP_SAMPLE_RATE_HZ;
        }

        let first = samples
            .first()
            .map(|sample| sample.device_timestamp.unwrap_or(sample.system_timestamp))
            .unwrap_or(0);
        let last = samples
            .last()
            .map(|sample| sample.device_timestamp.unwrap_or(sample.system_timestamp))
            .unwrap_or(first);
        let span_us = last.saturating_sub(first);
        if span_us <= 0 {
            return DEFAULT_ERRP_SAMPLE_RATE_HZ;
        }

        let rate_hz = (samples.len().saturating_sub(1) as f32 * 1_000_000.0) / span_us as f32;
        if rate_hz.is_finite() && (1.0..=2_048.0).contains(&rate_hz) {
            rate_hz
        } else {
            DEFAULT_ERRP_SAMPLE_RATE_HZ
        }
    }

    async fn validate_candidate_notification(
        &self,
        candidate: &CandidateModelReadyV2,
    ) -> std::result::Result<PathBuf, String> {
        if candidate.profile_id.trim().is_empty() {
            return Err("profile_id must not be empty".to_string());
        }
        if candidate.source_run_id.trim().is_empty() {
            return Err("source_run_id must not be empty".to_string());
        }

        candidate
            .manifest
            .validate_runtime_compatibility()
            .map_err(|e| format!("invalid manifest in notification payload: {e}"))?;
        candidate
            .metrics
            .validate()
            .map_err(|e| format!("invalid metrics in notification payload: {e}"))?;
        if candidate.metrics.generated_at < candidate.manifest.trained_at {
            return Err(format!(
                "metrics.generated_at {} precedes manifest.trained_at {}",
                candidate.metrics.generated_at, candidate.manifest.trained_at
            ));
        }

        let now_us = neurohid_types::now_micros();
        let max_future = now_us.saturating_add(MAX_CANDIDATE_FUTURE_SKEW_US);
        if candidate.created_at_us > max_future {
            return Err(format!(
                "created_at_us {} is too far in the future (now {})",
                candidate.created_at_us, now_us
            ));
        }
        if candidate.manifest.trained_at > max_future {
            return Err(format!(
                "manifest.trained_at {} is too far in the future (now {})",
                candidate.manifest.trained_at, now_us
            ));
        }
        if candidate.metrics.generated_at > max_future {
            return Err(format!(
                "metrics.generated_at {} is too far in the future (now {})",
                candidate.metrics.generated_at, now_us
            ));
        }

        let artifact_dir = PathBuf::from(&candidate.artifact_dir);
        if !artifact_dir.is_absolute() {
            return Err("artifact_dir must be an absolute path".to_string());
        }

        let canonical_dir = fs::canonicalize(&artifact_dir)
            .await
            .map_err(|e| format!("artifact_dir canonicalization failed: {e}"))?;
        let metadata = fs::metadata(&canonical_dir)
            .await
            .map_err(|e| format!("artifact_dir metadata lookup failed: {e}"))?;
        if !metadata.is_dir() {
            return Err(format!(
                "artifact_dir '{}' is not a directory",
                canonical_dir.display()
            ));
        }

        let allowed_roots = self.resolve_allowed_candidate_roots().await;
        let allowed = allowed_roots
            .iter()
            .any(|root| canonical_dir.starts_with(root));
        if !allowed {
            let roots = allowed_roots
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "artifact_dir '{}' is outside allowed roots [{}]",
                canonical_dir.display(),
                roots
            ));
        }

        self.validate_candidate_artifact_files(&canonical_dir, candidate)
            .await?;
        Ok(canonical_dir)
    }

    async fn resolve_allowed_candidate_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        if let Ok(temp) = fs::canonicalize(std::env::temp_dir()).await {
            roots.push(temp);
        }
        if let Ok(cwd) = std::env::current_dir() {
            if let Ok(canonical_cwd) = fs::canonicalize(cwd).await {
                roots.push(canonical_cwd);
            }
        }
        if let Some(store) = &self.profile_store {
            if let Ok(store_root) = fs::canonicalize(store.data_root()).await {
                roots.push(store_root);
            }
        }

        roots.sort();
        roots.dedup();
        if roots.is_empty() {
            roots.push(std::env::temp_dir());
        }
        roots
    }

    async fn validate_candidate_artifact_files(
        &self,
        artifact_dir: &Path,
        candidate: &CandidateModelReadyV2,
    ) -> std::result::Result<(), String> {
        let model_path = artifact_dir.join("decoder_candidate.onnx");
        let manifest_path = artifact_dir.join("decoder_candidate_manifest.json");
        let metrics_path = artifact_dir.join("decoder_candidate_metrics.json");

        let model_metadata = fs::metadata(&model_path)
            .await
            .map_err(|e| format!("candidate model metadata read failed: {e}"))?;
        if !model_metadata.is_file() {
            return Err(format!(
                "candidate model path '{}' is not a file",
                model_path.display()
            ));
        }
        if model_metadata.len() == 0 {
            return Err(format!(
                "candidate model '{}' is empty",
                model_path.display()
            ));
        }
        if model_metadata.len() > MAX_CANDIDATE_MODEL_BYTES {
            return Err(format!(
                "candidate model '{}' exceeds size limit {} bytes",
                model_path.display(),
                MAX_CANDIDATE_MODEL_BYTES
            ));
        }

        let manifest_payload = fs::read_to_string(&manifest_path)
            .await
            .map_err(|e| format!("candidate manifest read failed: {e}"))?;
        let metrics_payload = fs::read_to_string(&metrics_path)
            .await
            .map_err(|e| format!("candidate metrics read failed: {e}"))?;

        let manifest_from_dir =
            serde_json::from_str::<neurohid_types::model::ModelManifest>(&manifest_payload)
                .map_err(|e| format!("candidate manifest parse failed: {e}"))?;
        let metrics_from_dir = serde_json::from_str::<
            neurohid_types::learning::CandidateModelMetrics,
        >(&metrics_payload)
        .map_err(|e| format!("candidate metrics parse failed: {e}"))?;

        if manifest_from_dir != candidate.manifest {
            return Err(
                "candidate manifest payload does not match artifact manifest file".to_string(),
            );
        }
        if metrics_from_dir != candidate.metrics {
            return Err(
                "candidate metrics payload does not match artifact metrics file".to_string(),
            );
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
