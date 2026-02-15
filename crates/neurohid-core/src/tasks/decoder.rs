//! # Decoder Task
//!
//! Runs online inference in Rust using ONNX artifacts produced by the Python
//! training pipeline. This keeps the signal->action control loop local to Rust
//! so HID output is not gated on Python IPC availability.

use std::io::Cursor;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{RwLock, broadcast, mpsc};
use tract_onnx::prelude::*;

use neurohid_storage::ProfileStore;
use neurohid_types::{
    action::{Action, MouseAction, MouseButton, MouseButtonEvent, MouseMovement},
    config::DecoderConfig,
    control::RuntimeModeState,
    error::{DecoderError, Error, Result},
    learning::{CandidateGuardrails, TrainingEpisode},
    model::ModelManifest,
    observability::{self as obs, EmitGate, ObservabilityComponent, ObservabilityConfig},
    profile::ProfileId,
    signal::FeatureVector,
};

use crate::service::{DecoderCommand, ServiceState};
use crate::tasks::DecisionEventRecord;
use crate::tasks::latency::RollingLatency;
use crate::tasks::session_logger::EpisodeLogRecord;

const LATENCY_WINDOW_SIZE: usize = 512;
const DECODER_SUMMARY_EVERY_DECISIONS: u64 = 256;

type OnnxPlan = SimplePlan<TypedFact, Box<dyn TypedOp>, TypedModel>;

#[derive(Clone)]
struct LoadedModel {
    manifest: ModelManifest,
    model: Arc<dyn InferenceModel>,
}

trait InferenceModel: Send + Sync {
    fn infer(&self, normalized: &[f32]) -> Result<Vec<f32>>;
}

struct OnnxInferenceModel {
    model: OnnxPlan,
}

impl InferenceModel for OnnxInferenceModel {
    fn infer(&self, normalized: &[f32]) -> Result<Vec<f32>> {
        let input =
            tract_ndarray::Array2::from_shape_vec((1, normalized.len()), normalized.to_vec())
                .map_err(|e| Error::Decoder(DecoderError::InferenceFailed(e.to_string())))?;
        let input = input.into_tensor();
        let output = self
            .model
            .run(tvec!(input.into()))
            .map_err(|e| Error::Decoder(DecoderError::InferenceFailed(e.to_string())))?;
        let first = output.first().ok_or_else(|| {
            Error::Decoder(DecoderError::InferenceFailed(
                "empty model output".to_string(),
            ))
        })?;
        first
            .to_array_view::<f32>()
            .map_err(|e| Error::Decoder(DecoderError::InferenceFailed(e.to_string())))
            .map(|array| array.iter().copied().collect())
    }
}

#[async_trait]
trait ArtifactLoader: Send + Sync {
    async fn load(
        &self,
        profile_store: Option<&ProfileStore>,
        profile_id: &ProfileId,
    ) -> Result<LoadedModel>;
}

struct OnnxArtifactLoader;

#[async_trait]
impl ArtifactLoader for OnnxArtifactLoader {
    async fn load(
        &self,
        profile_store: Option<&ProfileStore>,
        profile_id: &ProfileId,
    ) -> Result<LoadedModel> {
        let store = profile_store.as_ref().ok_or_else(|| {
            Error::Decoder(DecoderError::ModelFileError(
                "profile store unavailable for decoder model load".to_string(),
            ))
        })?;

        let manifest = store.load_decoder_manifest(profile_id).await?;
        manifest.validate_runtime_compatibility().map_err(|msg| {
            Error::Decoder(DecoderError::ModelFileError(format!(
                "manifest compatibility check failed: {msg}"
            )))
        })?;

        let model_bytes = store.load_decoder_model_onnx(profile_id).await?;
        let model = load_onnx_model(&model_bytes)?;

        Ok(LoadedModel { manifest, model })
    }
}

/// Decoder task for Rust-native ONNX inference.
pub struct DecoderTask {
    #[allow(dead_code)]
    config: DecoderConfig,
    feature_rx: mpsc::Receiver<FeatureVector>,
    action_tx: mpsc::Sender<Action>,
    state: Arc<RwLock<ServiceState>>,
    profile_store: Option<ProfileStore>,
    active_profile_id: Option<ProfileId>,
    decoder_command_rx: Option<mpsc::Receiver<DecoderCommand>>,
    decision_event_tx: Option<mpsc::Sender<DecisionEventRecord>>,
    episode_log_tx: Option<mpsc::Sender<EpisodeLogRecord>>,
    active_model: Option<LoadedModel>,
    loader: Arc<dyn ArtifactLoader>,
    decode_latency: RollingLatency,
    fallback_enabled: bool,
    decision_sequence: u64,
    emit_gate: EmitGate,
}

impl DecoderTask {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: DecoderConfig,
        feature_rx: mpsc::Receiver<FeatureVector>,
        action_tx: mpsc::Sender<Action>,
        state: Arc<RwLock<ServiceState>>,
        profile_store: Option<ProfileStore>,
        profile_id: Option<ProfileId>,
        decoder_command_rx: Option<mpsc::Receiver<DecoderCommand>>,
        decision_event_tx: Option<mpsc::Sender<DecisionEventRecord>>,
        episode_log_tx: Option<mpsc::Sender<EpisodeLogRecord>>,
        fallback_enabled: bool,
        observability: ObservabilityConfig,
    ) -> Self {
        Self::new_inner(
            config,
            feature_rx,
            action_tx,
            state,
            profile_store,
            profile_id,
            decoder_command_rx,
            decision_event_tx,
            episode_log_tx,
            fallback_enabled,
            Arc::new(OnnxArtifactLoader),
            observability,
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn new_with_loader(
        config: DecoderConfig,
        feature_rx: mpsc::Receiver<FeatureVector>,
        action_tx: mpsc::Sender<Action>,
        state: Arc<RwLock<ServiceState>>,
        profile_store: Option<ProfileStore>,
        profile_id: Option<ProfileId>,
        decoder_command_rx: Option<mpsc::Receiver<DecoderCommand>>,
        decision_event_tx: Option<mpsc::Sender<DecisionEventRecord>>,
        episode_log_tx: Option<mpsc::Sender<EpisodeLogRecord>>,
        fallback_enabled: bool,
        loader: Arc<dyn ArtifactLoader>,
        observability: ObservabilityConfig,
    ) -> Self {
        Self::new_inner(
            config,
            feature_rx,
            action_tx,
            state,
            profile_store,
            profile_id,
            decoder_command_rx,
            decision_event_tx,
            episode_log_tx,
            fallback_enabled,
            loader,
            observability,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_inner(
        config: DecoderConfig,
        feature_rx: mpsc::Receiver<FeatureVector>,
        action_tx: mpsc::Sender<Action>,
        state: Arc<RwLock<ServiceState>>,
        profile_store: Option<ProfileStore>,
        profile_id: Option<ProfileId>,
        decoder_command_rx: Option<mpsc::Receiver<DecoderCommand>>,
        decision_event_tx: Option<mpsc::Sender<DecisionEventRecord>>,
        episode_log_tx: Option<mpsc::Sender<EpisodeLogRecord>>,
        fallback_enabled: bool,
        loader: Arc<dyn ArtifactLoader>,
        observability: ObservabilityConfig,
    ) -> Self {
        Self {
            config,
            feature_rx,
            action_tx,
            state,
            profile_store,
            active_profile_id: profile_id,
            decoder_command_rx,
            decision_event_tx,
            episode_log_tx,
            active_model: None,
            loader,
            decode_latency: RollingLatency::new(LATENCY_WINDOW_SIZE),
            fallback_enabled,
            decision_sequence: 0,
            emit_gate: EmitGate::new(observability.policy_for(ObservabilityComponent::Decoder)),
        }
    }

    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        tracing::info!(
            event = obs::event::TASK_STARTED,
            span = obs::span::DECODER_RUN,
            stage = obs::stage::DECODER,
            "Decoder task started"
        );

        self.switch_profile(self.active_profile_id.clone()).await;

        loop {
            loop {
                let next_command = {
                    let Some(rx) = &mut self.decoder_command_rx else {
                        break;
                    };
                    rx.try_recv().ok()
                };
                let Some(cmd) = next_command else {
                    break;
                };

                match cmd {
                    DecoderCommand::ReloadModel => self.reload_model().await,
                    DecoderCommand::PromoteCandidateModel => self.promote_candidate_model().await,
                    DecoderCommand::SetActiveProfile { profile_id } => {
                        self.switch_profile(profile_id).await;
                    }
                }
            }

            tokio::select! {
                _ = shutdown.recv() => {
                    tracing::info!(event = obs::event::TASK_STOPPED, "Decoder task received shutdown signal");
                    break;
                }
                feature = self.feature_rx.recv() => {
                    let Some(feature) = feature else {
                        tracing::info!("Decoder input feature channel closed");
                        break;
                    };

                    if tracing::enabled!(tracing::Level::DEBUG) && self.emit_gate.allow_debug() {
                        tracing::debug!(
                            event = obs::event::FEATURE_WINDOW_EMITTED,
                            decision_id = obs::field::UNKNOWN,
                            stream_id = feature.stream_id.as_deref().unwrap_or("__default__"),
                            feature_timestamp_us = feature.timestamp,
                            feature_dim = feature.dim(),
                            "Decoder received feature vector"
                        );
                    }

                    let inference = if let Some(model) = self.active_model.clone() {
                        run_inference(&model, &feature).map(|action| {
                            (
                                action,
                                Some(model.manifest.model_version.clone()),
                                "onnx".to_string(),
                            )
                        })
                    } else if self.fallback_enabled {
                        Ok((
                            lightweight_fallback_action(&feature),
                            Some("lightweight-rust".to_string()),
                            "lightweight_rust".to_string(),
                        ))
                    } else {
                        continue;
                    };

                    match inference {
                        Ok((mut action, model_version, model_kind)) => {
                            self.decision_sequence = self.decision_sequence.saturating_add(1);
                            let decision_id = format!("dec_{}", self.decision_sequence);
                            action.decision_id = Some(decision_id.clone());
                            self.record_decode_latency(feature.timestamp).await;
                            if action.is_none() {
                                continue;
                            }
                            self.log_episode_with_version(&feature, &action, model_version.clone())
                                .await;
                            self.forward_decision_event(
                                &decision_id,
                                &feature,
                                &action,
                                model_version.clone(),
                                feature.stream_id.as_deref(),
                            )
                            .await;

                            if tracing::enabled!(tracing::Level::DEBUG) && self.emit_gate.allow_debug() {
                                tracing::debug!(
                                    event = obs::event::DECISION_EMITTED,
                                    decision_id = %decision_id,
                                    stream_id = feature.stream_id.as_deref().unwrap_or("__default__"),
                                    confidence = action.confidence,
                                    model_kind = %model_kind,
                                    "Decoder emitted decision"
                                );
                            }

                            if self.decision_sequence.is_multiple_of(DECODER_SUMMARY_EVERY_DECISIONS)
                                && self.emit_gate.allow_info()
                            {
                                tracing::info!(
                                    event = obs::event::TASK_SUMMARY,
                                    decision_id = obs::field::UNKNOWN,
                                    stream_id = obs::field::UNKNOWN,
                                    decisions_emitted = self.decision_sequence,
                                    model_kind = %model_kind,
                                    "Decoder periodic summary"
                                );
                            }

                            if self.action_tx.send(action).await.is_err() {
                                tracing::warn!("Action receiver dropped");
                                break;
                            }
                            self.set_runtime_mode_for_model_kind(&model_kind).await;
                        },
                        Err(err) => {
                            tracing::warn!("Decoder inference failed: {}", err);
                            self.set_runtime_mode_for_model_kind("none").await;
                        }
                    }
                }
            }
        }

        self.set_decoder_status(None).await;
        tracing::info!(event = obs::event::TASK_STOPPED, "Decoder task stopped");
        Ok(())
    }

    async fn reload_model(&mut self) {
        let Some(profile_id) = self.active_profile_id.clone() else {
            tracing::debug!("Ignoring model reload without active profile");
            self.set_decoder_status(None).await;
            return;
        };

        match self.load_model_for_profile(&profile_id).await {
            Ok(model) => {
                self.active_model = Some(model);
                self.set_decoder_status(self.active_model.as_ref()).await;
                tracing::info!("Reloaded decoder model for profile {}", profile_id);
            }
            Err(err) => {
                tracing::warn!(
                    "Decoder reload failed for profile {} (keeping last-known-good model): {}",
                    profile_id,
                    err
                );
                self.set_decoder_status(self.active_model.as_ref()).await;
            }
        }
    }

    async fn promote_candidate_model(&mut self) {
        let Some(profile_id) = self.active_profile_id.clone() else {
            tracing::warn!("Ignoring candidate promotion without active profile");
            self.record_candidate_outcome(
                false,
                "candidate promotion rejected: no active profile".to_string(),
            )
            .await;
            return;
        };
        let Some(store) = self.profile_store.clone() else {
            tracing::warn!(
                "Ignoring candidate promotion for profile {} without profile store",
                profile_id
            );
            self.record_candidate_outcome(
                false,
                format!(
                    "candidate promotion rejected for profile {}: profile store unavailable",
                    profile_id
                ),
            )
            .await;
            return;
        };

        match self.validate_candidate_model(&store, &profile_id).await {
            Ok(()) => {}
            Err(reason) => {
                tracing::warn!(
                    "Rejected candidate model for profile {} due to guardrail failure: {}",
                    profile_id,
                    reason
                );
                self.record_candidate_outcome(
                    false,
                    format!(
                        "candidate promotion rejected for profile {}: {}",
                        profile_id, reason
                    ),
                )
                .await;
                return;
            }
        }

        let previous_active_artifacts = self
            .backup_active_decoder_artifacts(&store, &profile_id)
            .await;

        if let Err(error) = store.promote_decoder_candidate(&profile_id).await {
            tracing::warn!(
                "Failed to promote candidate model for profile {}: {}",
                profile_id,
                error
            );
            self.record_candidate_outcome(
                false,
                format!(
                    "candidate promotion failed for profile {}: {}",
                    profile_id, error
                ),
            )
            .await;
            return;
        }

        match self.load_model_for_profile(&profile_id).await {
            Ok(model) => {
                self.active_model = Some(model);
                self.set_decoder_status(self.active_model.as_ref()).await;
                tracing::info!("Promoted candidate model for profile {}", profile_id);
            }
            Err(error) => {
                tracing::warn!(
                    "Candidate model promotion activation failed for profile {}: {}. Attempting rollback.",
                    profile_id,
                    error
                );

                if let Some((previous_model_bytes, previous_manifest)) = previous_active_artifacts {
                    if let Err(restore_error) = store
                        .save_decoder_model_onnx(&profile_id, &previous_model_bytes)
                        .await
                    {
                        tracing::error!(
                            "Rollback failed restoring previous decoder model for profile {}: {}",
                            profile_id,
                            restore_error
                        );
                        self.set_decoder_status(self.active_model.as_ref()).await;
                        return;
                    }

                    if let Err(restore_error) = store
                        .save_decoder_manifest(&profile_id, &previous_manifest)
                        .await
                    {
                        tracing::error!(
                            "Rollback failed restoring previous decoder manifest for profile {}: {}",
                            profile_id,
                            restore_error
                        );
                        self.set_decoder_status(self.active_model.as_ref()).await;
                        return;
                    }

                    match self.load_model_for_profile(&profile_id).await {
                        Ok(model) => {
                            self.active_model = Some(model);
                            self.set_decoder_status(self.active_model.as_ref()).await;
                            tracing::info!(
                                "Rollback restored previous decoder model for profile {}",
                                profile_id
                            );
                        }
                        Err(reload_error) => {
                            tracing::error!(
                                "Rollback artifacts restored for profile {}, but reload failed: {}",
                                profile_id,
                                reload_error
                            );
                            self.set_decoder_status(self.active_model.as_ref()).await;
                        }
                    }
                } else {
                    tracing::warn!(
                        "No previous active artifacts available for rollback on profile {}",
                        profile_id
                    );
                    self.set_decoder_status(self.active_model.as_ref()).await;
                }

                self.record_candidate_outcome(
                    false,
                    format!(
                        "candidate promotion failed activation for profile {} and was rejected",
                        profile_id
                    ),
                )
                .await;

                return;
            }
        }

        self.record_candidate_outcome(
            true,
            format!("candidate promotion succeeded for profile {}", profile_id),
        )
        .await;

        if let Err(error) = store.clear_decoder_candidate(&profile_id).await {
            tracing::warn!(
                "Failed to clear candidate artifacts after promotion for profile {}: {}",
                profile_id,
                error
            );
        }
    }

    async fn switch_profile(&mut self, profile_id: Option<ProfileId>) {
        self.active_profile_id = profile_id.clone();
        let Some(profile_id) = profile_id else {
            self.active_model = None;
            self.set_decoder_status(None).await;
            return;
        };

        match self.load_model_for_profile(&profile_id).await {
            Ok(model) => {
                self.active_model = Some(model);
                self.set_decoder_status(self.active_model.as_ref()).await;
                tracing::info!("Loaded decoder model for profile {}", profile_id);
            }
            Err(err) => {
                self.active_model = None;
                self.set_decoder_status(None).await;
                tracing::warn!(
                    "No compatible decoder model available for profile {}: {}",
                    profile_id,
                    err
                );
            }
        }
    }

    async fn load_model_for_profile(&self, profile_id: &ProfileId) -> Result<LoadedModel> {
        self.loader
            .load(self.profile_store.as_ref(), profile_id)
            .await
    }

    async fn validate_candidate_model(
        &self,
        store: &ProfileStore,
        profile_id: &ProfileId,
    ) -> std::result::Result<(), String> {
        let manifest = store
            .load_decoder_candidate_manifest(profile_id)
            .await
            .map_err(|e| e.to_string())?;
        manifest.validate_runtime_compatibility()?;

        let model_bytes = store
            .load_decoder_candidate_model_onnx(profile_id)
            .await
            .map_err(|e| e.to_string())?;
        let _validated_model = load_onnx_model(&model_bytes).map_err(|e| e.to_string())?;

        let metrics = store
            .load_decoder_candidate_metrics(profile_id)
            .await
            .map_err(|e| e.to_string())?;
        let guardrails = CandidateGuardrails::default();
        metrics.evaluate(&guardrails)?;

        let current_decode_p95 = self.state.read().await.decode_latency_p95_us;
        if current_decode_p95 > 0
            && metrics.decode_latency_p95_us > current_decode_p95.saturating_mul(2)
        {
            return Err(format!(
                "candidate decode latency p95 {} us exceeds runtime baseline {} us by >2x",
                metrics.decode_latency_p95_us, current_decode_p95
            ));
        }
        Ok(())
    }

    async fn backup_active_decoder_artifacts(
        &self,
        store: &ProfileStore,
        profile_id: &ProfileId,
    ) -> Option<(Vec<u8>, ModelManifest)> {
        let active_model_bytes = match store.load_decoder_model_onnx(profile_id).await {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::debug!(
                    "No active decoder ONNX artifacts to back up for profile {}: {}",
                    profile_id,
                    error
                );
                return None;
            }
        };

        let active_manifest = match store.load_decoder_manifest(profile_id).await {
            Ok(manifest) => manifest,
            Err(error) => {
                tracing::debug!(
                    "No active decoder manifest to back up for profile {}: {}",
                    profile_id,
                    error
                );
                return None;
            }
        };

        Some((active_model_bytes, active_manifest))
    }

    async fn set_decoder_status(&self, loaded: Option<&LoadedModel>) {
        let mut state = self.state.write().await;
        state.decoder_ready = loaded.is_some();
        state.decoder_model_version = loaded.map(|m| m.manifest.model_version.clone());
        if loaded.is_some() {
            state.fallback_model_kind = Some("onnx".to_string());
            state.runtime_mode_state = RuntimeModeState::Full;
        } else if self.fallback_enabled {
            state.fallback_model_kind = Some("lightweight_rust".to_string());
            state.runtime_mode_state = RuntimeModeState::Fallback;
        } else {
            state.fallback_model_kind = Some("none".to_string());
            state.runtime_mode_state = RuntimeModeState::Degraded;
        }
    }

    async fn record_decode_latency(&mut self, feature_timestamp: i64) {
        if feature_timestamp <= 0 {
            return;
        }
        let now_micros = neurohid_types::now_micros();
        let latency_us = now_micros.saturating_sub(feature_timestamp) as u64;
        self.decode_latency.record(latency_us);
        let mut state = self.state.write().await;
        state.decode_latency_last_us = self.decode_latency.last_us();
        state.decode_latency_p95_us = self.decode_latency.p95_us();
    }

    async fn log_episode_with_version(
        &self,
        feature: &FeatureVector,
        action: &Action,
        model_version: Option<String>,
    ) {
        let Some(tx) = &self.episode_log_tx else {
            return;
        };
        let Some(profile_id) = self.active_profile_id.clone() else {
            return;
        };

        let signal_quality = self.state.read().await.signal_quality;
        let episode = TrainingEpisode {
            timestamp: feature.timestamp,
            feature_values: feature.values.clone(),
            action: action.clone(),
            decoder_confidence: action.confidence,
            signal_quality,
            decoder_model_version: model_version,
            errp_error_probability: None,
            errp_confidence: None,
        };
        if let Err(error) = tx.try_send(EpisodeLogRecord {
            profile_id,
            episode,
        }) {
            tracing::trace!(
                "Dropped training episode due to logger backpressure: {}",
                error
            );
        }
    }

    async fn forward_decision_event(
        &self,
        decision_id: &str,
        feature: &FeatureVector,
        action: &Action,
        model_version: Option<String>,
        stream_id: Option<&str>,
    ) {
        let Some(tx) = &self.decision_event_tx else {
            return;
        };
        let signal_quality = self.state.read().await.signal_quality;
        let event = DecisionEventRecord {
            decision_id: decision_id.to_string(),
            timestamp_us: feature.timestamp,
            feature_values: feature.values.clone(),
            action: action.clone(),
            decoder_confidence: action.confidence,
            signal_quality,
            decoder_model_version: model_version,
            stream_id: stream_id.map(std::borrow::ToOwned::to_owned),
        };
        if let Err(error) = tx.try_send(event) {
            tracing::trace!(
                "Dropped decision event due to ML bridge backpressure: {}",
                error
            );
        }
    }

    async fn set_runtime_mode_for_model_kind(&self, model_kind: &str) {
        let mut state = self.state.write().await;
        state.fallback_model_kind = Some(model_kind.to_string());
        state.runtime_mode_state = match model_kind {
            "onnx" => {
                if state.ml_bridge_connected && !state.ml_bridge_stalled {
                    RuntimeModeState::Full
                } else {
                    RuntimeModeState::Fallback
                }
            }
            "lightweight_rust" => RuntimeModeState::Fallback,
            _ => RuntimeModeState::Degraded,
        };
    }

    async fn record_candidate_outcome(&self, succeeded: bool, message: String) {
        let mut state = self.state.write().await;
        if succeeded {
            state.candidate_promotions_succeeded =
                state.candidate_promotions_succeeded.saturating_add(1);
        } else {
            state.candidate_promotions_rejected =
                state.candidate_promotions_rejected.saturating_add(1);
        }
        state.candidate_last_outcome = Some(message);
    }
}

fn load_onnx_model(bytes: &[u8]) -> Result<Arc<dyn InferenceModel>> {
    let mut cursor = Cursor::new(bytes);
    let model = tract_onnx::onnx()
        .model_for_read(&mut cursor)
        .map_err(|e| Error::Decoder(DecoderError::ModelFileError(e.to_string())))?
        .into_optimized()
        .map_err(|e| Error::Decoder(DecoderError::ModelFileError(e.to_string())))?
        .into_runnable()
        .map_err(|e| Error::Decoder(DecoderError::ModelFileError(e.to_string())))?;
    Ok(Arc::new(OnnxInferenceModel { model }))
}

fn run_inference(model: &LoadedModel, feature: &FeatureVector) -> Result<Action> {
    if feature.dim() != model.manifest.input_dim {
        return Err(Error::Decoder(DecoderError::InvalidInputDimensions {
            expected: model.manifest.input_dim,
            got: feature.dim(),
        }));
    }

    let normalized: Vec<f32> = feature
        .values
        .iter()
        .zip(
            model
                .manifest
                .normalization_stats
                .mean
                .iter()
                .zip(model.manifest.normalization_stats.std.iter()),
        )
        .map(|(value, (mean, std))| ((*value - *mean) / *std).clamp(-10.0, 10.0))
        .collect();

    let values = model.model.infer(&normalized)?;
    Ok(action_from_output(&values, feature.timestamp))
}

fn lightweight_fallback_action(feature: &FeatureVector) -> Action {
    let dx = feature
        .values
        .first()
        .copied()
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);
    let dy = feature
        .values
        .get(1)
        .copied()
        .unwrap_or(0.0)
        .clamp(-1.0, 1.0);
    let confidence = feature
        .values
        .get(2)
        .copied()
        .map(to_probability)
        .unwrap_or_else(|| (dx.abs() + dy.abs()).clamp(0.0, 1.0));
    let mut action = Action::none().with_confidence(confidence);
    if dx.abs() > 0.01 || dy.abs() > 0.01 {
        action.mouse = Some(MouseAction::move_relative(dx, dy));
        action.timestamp = feature.timestamp;
    }
    action
}

fn action_from_output(values: &[f32], timestamp: i64) -> Action {
    let dx = *values.first().unwrap_or(&0.0);
    let dy = *values.get(1).unwrap_or(&0.0);

    let (left_click_prob, right_click_prob, confidence_raw) = match values.len() {
        0..=2 => (None, None, None),
        3 => (None, None, values.get(2).copied()),
        4 => (values.get(2).copied(), None, values.get(3).copied()),
        _ => (
            values.get(2).copied(),
            values.get(3).copied(),
            values.get(4).copied(),
        ),
    };

    let confidence = confidence_raw
        .map(to_probability)
        .unwrap_or_else(|| (dx.abs() + dy.abs()).clamp(0.0, 1.0));

    let mut mouse = MouseAction {
        movement: None,
        buttons: Vec::new(),
        scroll: None,
    };

    if dx.abs() > 0.01 || dy.abs() > 0.01 {
        mouse.movement = Some(MouseMovement { dx, dy });
    }

    if left_click_prob.is_some_and(|p| to_probability(p) >= 0.8) {
        mouse.buttons.push(MouseButtonEvent {
            button: MouseButton::Left,
            pressed: true,
        });
        mouse.buttons.push(MouseButtonEvent {
            button: MouseButton::Left,
            pressed: false,
        });
    }

    if right_click_prob.is_some_and(|p| to_probability(p) >= 0.8) {
        mouse.buttons.push(MouseButtonEvent {
            button: MouseButton::Right,
            pressed: true,
        });
        mouse.buttons.push(MouseButtonEvent {
            button: MouseButton::Right,
            pressed: false,
        });
    }

    let mouse = if mouse.movement.is_some() || !mouse.buttons.is_empty() {
        Some(mouse)
    } else {
        None
    };

    Action {
        timestamp,
        mouse,
        keyboard: None,
        confidence,
        decision_id: None,
    }
}

fn to_probability(value: f32) -> f32 {
    if (0.0..=1.0).contains(&value) {
        value
    } else {
        1.0 / (1.0 + (-value).exp())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use tokio::sync::{RwLock, mpsc};

    use neurohid_types::action::MouseButton;
    use neurohid_types::model::{
        CURRENT_ACTION_SCHEMA_VERSION, CURRENT_FEATURE_SCHEMA_VERSION, ModelManifest,
        NormalizationStats,
    };

    use super::{
        ArtifactLoader, DecoderTask, InferenceModel, LoadedModel, action_from_output,
        to_probability,
    };
    use crate::service::ServiceState;

    #[derive(Clone)]
    struct FixedInferenceModel {
        output: Vec<f32>,
    }

    impl InferenceModel for FixedInferenceModel {
        fn infer(&self, _normalized: &[f32]) -> neurohid_types::error::Result<Vec<f32>> {
            Ok(self.output.clone())
        }
    }

    #[derive(Clone)]
    enum FakeLoadOutcome {
        Success {
            version: String,
            input_dim: usize,
            output: Vec<f32>,
        },
        Failure(String),
    }

    #[derive(Default)]
    struct FakeArtifactLoader {
        outcomes: RwLock<HashMap<String, FakeLoadOutcome>>,
    }

    impl FakeArtifactLoader {
        async fn set_success(
            &self,
            profile_id: &str,
            version: &str,
            input_dim: usize,
            output: Vec<f32>,
        ) {
            self.outcomes.write().await.insert(
                profile_id.to_string(),
                FakeLoadOutcome::Success {
                    version: version.to_string(),
                    input_dim,
                    output,
                },
            );
        }

        async fn set_failure(&self, profile_id: &str, reason: &str) {
            self.outcomes.write().await.insert(
                profile_id.to_string(),
                FakeLoadOutcome::Failure(reason.to_string()),
            );
        }
    }

    #[async_trait::async_trait]
    impl ArtifactLoader for FakeArtifactLoader {
        async fn load(
            &self,
            _profile_store: Option<&neurohid_storage::ProfileStore>,
            profile_id: &neurohid_types::profile::ProfileId,
        ) -> neurohid_types::error::Result<LoadedModel> {
            let Some(outcome) = self.outcomes.read().await.get(&profile_id.0).cloned() else {
                return Err(neurohid_types::error::Error::Decoder(
                    neurohid_types::error::DecoderError::ModelFileError(
                        "missing fake loader outcome".to_string(),
                    ),
                ));
            };

            match outcome {
                FakeLoadOutcome::Success {
                    version,
                    input_dim,
                    output,
                } => Ok(LoadedModel {
                    manifest: ModelManifest {
                        model_version: version,
                        input_dim,
                        feature_schema_version: CURRENT_FEATURE_SCHEMA_VERSION,
                        action_schema_version: CURRENT_ACTION_SCHEMA_VERSION,
                        normalization_stats: NormalizationStats {
                            mean: vec![0.0; input_dim],
                            std: vec![1.0; input_dim],
                        },
                        trained_at: 1,
                    },
                    model: Arc::new(FixedInferenceModel { output }),
                }),
                FakeLoadOutcome::Failure(reason) => Err(neurohid_types::error::Error::Decoder(
                    neurohid_types::error::DecoderError::ModelFileError(reason),
                )),
            }
        }
    }

    fn make_task_with_loader(
        loader: Arc<dyn ArtifactLoader>,
    ) -> (DecoderTask, Arc<RwLock<ServiceState>>) {
        let (_feature_tx, feature_rx) = mpsc::channel(8);
        let (action_tx, _action_rx) = mpsc::channel(8);
        let state = Arc::new(RwLock::new(ServiceState::default()));
        let task = DecoderTask::new_with_loader(
            neurohid_types::config::DecoderConfig::default(),
            feature_rx,
            action_tx,
            Arc::clone(&state),
            None,
            None,
            None,
            None,
            None,
            true,
            loader,
            neurohid_types::observability::ObservabilityConfig::default(),
        );
        (task, state)
    }

    #[tokio::test]
    async fn reload_keeps_last_known_good_model_on_failure() {
        let loader = Arc::new(FakeArtifactLoader::default());
        loader
            .set_success("profile_a", "1.0.0", 4, vec![0.1, 0.2, 0.0, 0.0, 0.9])
            .await;

        let (mut task, state) = make_task_with_loader(loader.clone());
        task.switch_profile(Some(neurohid_types::profile::ProfileId::new("profile_a")))
            .await;
        assert_eq!(
            task.active_model
                .as_ref()
                .map(|m| m.manifest.model_version.as_str()),
            Some("1.0.0")
        );

        loader
            .set_failure("profile_a", "broken model bytes on reload")
            .await;
        task.reload_model().await;

        assert_eq!(
            task.active_model
                .as_ref()
                .map(|m| m.manifest.model_version.as_str()),
            Some("1.0.0")
        );
        let state_guard = state.read().await;
        assert!(state_guard.decoder_ready);
        assert_eq!(state_guard.decoder_model_version.as_deref(), Some("1.0.0"));
    }

    #[tokio::test]
    async fn profile_switch_loads_target_model() {
        let loader = Arc::new(FakeArtifactLoader::default());
        loader
            .set_success("profile_a", "1.0.0", 4, vec![0.1, 0.2, 0.0, 0.0, 0.9])
            .await;
        loader
            .set_success("profile_b", "2.0.0", 4, vec![0.3, 0.4, 0.0, 0.0, 0.95])
            .await;

        let (mut task, state) = make_task_with_loader(loader);
        task.switch_profile(Some(neurohid_types::profile::ProfileId::new("profile_a")))
            .await;
        task.switch_profile(Some(neurohid_types::profile::ProfileId::new("profile_b")))
            .await;

        assert_eq!(
            task.active_model
                .as_ref()
                .map(|m| m.manifest.model_version.as_str()),
            Some("2.0.0")
        );

        {
            let state_guard = state.read().await;
            assert!(state_guard.decoder_ready);
            assert_eq!(state_guard.decoder_model_version.as_deref(), Some("2.0.0"));
        }

        task.switch_profile(None).await;
        let state_guard = state.read().await;
        assert!(!state_guard.decoder_ready);
        assert!(state_guard.decoder_model_version.is_none());
    }

    #[test]
    fn output_mapping_supports_movement_only() {
        let action = action_from_output(&[0.2, -0.1], 123);
        assert!(action.mouse.is_some());
        assert!(action.confidence > 0.0);
        assert_eq!(action.timestamp, 123);
    }

    #[test]
    fn output_mapping_supports_click_and_confidence() {
        let action = action_from_output(&[0.0, 0.0, 1.0, 0.0, 0.95], 42);
        let mouse = action.mouse.expect("mouse action expected");
        assert_eq!(mouse.buttons.len(), 2);
        assert_eq!(mouse.buttons[0].button, MouseButton::Left);
        assert!(action.confidence > 0.9);
        assert_eq!(action.timestamp, 42);
    }

    #[test]
    fn probability_helper_handles_logits() {
        assert!(to_probability(4.0) > 0.9);
        assert!(to_probability(-4.0) < 0.1);
        assert_eq!(to_probability(0.8), 0.8);
    }
}
