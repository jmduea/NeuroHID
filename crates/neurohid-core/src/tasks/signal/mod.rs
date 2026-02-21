//! # Signal Task
//!
//! This task sits between the device and the decoder. It receives raw EEG
//! samples, applies digital filters to clean up the signal, and extracts
//! features that the neural network can understand.
//!
//! Think of it like a translator: the device speaks in raw voltage readings,
//! but the decoder needs higher-level summaries like "how much alpha rhythm
//! is present right now?" The signal task does that translation.
//!
//! Supports multiple concurrent streams — each stream gets its own sample
//! buffer and independent feature extraction. Features from all streams are
//! merged into a single output channel.

mod config;
mod pipeline_task;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast, mpsc};

use async_trait::async_trait;
use neurohid_types::{
    SignalChannels, SignalPreprocessor,
    config::SignalConfig,
    device::DiscoveredStream,
    error::Result,
    event::{MarkerPayload, MarkerType, StreamMarker},
    observability::{self as obs, ObservabilityComponent, ObservabilityConfig},
    reward::ErrPResult,
    signal::{FeatureVector, Sample},
};

use crate::observability::EmitGate;
use crate::service::{IntegrityStage, ServiceState, SignalCommand};
use crate::tasks::latency::RollingLatency;

use config::{
    DEFAULT_STREAM_KEY, SIGNAL_FEATURE_DEBUG_EVERY_SAMPLES, SIGNAL_SUMMARY_EVERY_SAMPLES,
};
use pipeline_task::{StreamBuffer, StreamRuntimeMetrics, SignalSequenceIssue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamRoute {
    Eeg,
    Motion,
    Auxiliary,
    Unknown,
}

/// The signal processing task.
pub struct SignalTask {
    config: SignalConfig,
    sample_rx: mpsc::Receiver<Sample>,
    feature_tx: mpsc::Sender<FeatureVector>,
    errp_rx: mpsc::Receiver<ErrPResult>,
    state: Arc<RwLock<ServiceState>>,
    signal_command_rx: Option<mpsc::Receiver<SignalCommand>>,
    sample_tap_tx: Option<mpsc::Sender<Sample>>,

    /// Broadcast channel for forwarding raw samples to hub visualization widgets.
    sample_broadcast_tx: Option<broadcast::Sender<Sample>>,
    /// Broadcast channel for forwarding features to hub visualization widgets.
    feature_broadcast_tx: Option<broadcast::Sender<FeatureVector>>,
    /// Broadcast channel for forwarding timeline markers.
    marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,

    // Internal state for signal processing
    stream_buffers: HashMap<String, StreamBuffer>,
    sample_count: u64,
    last_head_movement_marker_us: i64,
    head_movement_threshold: f32,
    signal_latency: RollingLatency,
    emit_gate: EmitGate,
    stream_routes: HashMap<String, StreamRoute>,
}

impl SignalTask {
    /// Creates a new signal task.
    #[expect(
        clippy::too_many_arguments,
        reason = "Task constructor wires signal, ErrP, broadcast fan-out, and observability channels"
    )]
    pub fn new(
        config: SignalConfig,
        sample_rx: mpsc::Receiver<Sample>,
        feature_tx: mpsc::Sender<FeatureVector>,
        errp_rx: mpsc::Receiver<ErrPResult>,
        state: Arc<RwLock<ServiceState>>,
        signal_command_rx: Option<mpsc::Receiver<SignalCommand>>,
        sample_tap_tx: Option<mpsc::Sender<Sample>>,
        sample_broadcast_tx: Option<broadcast::Sender<Sample>>,
        feature_broadcast_tx: Option<broadcast::Sender<FeatureVector>>,
        marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,
        observability: ObservabilityConfig,
    ) -> Self {
        Self {
            config,
            sample_rx,
            feature_tx,
            errp_rx,
            state,
            signal_command_rx,
            sample_tap_tx,
            sample_broadcast_tx,
            feature_broadcast_tx,
            marker_broadcast_tx,
            stream_buffers: HashMap::new(),
            sample_count: 0,
            last_head_movement_marker_us: 0,
            head_movement_threshold: 0.4,
            signal_latency: RollingLatency::new(512),
            emit_gate: EmitGate::new(observability.policy_for(ObservabilityComponent::Signal)),
            stream_routes: HashMap::new(),
        }
    }

    fn classify_stream_route(
        stream_key: &str,
        discovered: Option<&DiscoveredStream>,
        sample: &Sample,
    ) -> StreamRoute {
        let stream_name = discovered.map_or_else(String::new, |s| s.name.to_ascii_lowercase());
        let stream_type =
            discovered.map_or_else(String::new, |s| s.stream_type.to_ascii_lowercase());
        let stream_key_lower = stream_key.to_ascii_lowercase();
        let combined = format!("{stream_type} {stream_name} {stream_key_lower}");

        let channel_count = discovered
            .map(|s| s.channel_count.max(0) as usize)
            .unwrap_or_else(|| sample.values.len());

        let is_motion = ["motion", "acc", "imu", "gyro"]
            .iter()
            .any(|token| combined.contains(token));
        if is_motion {
            return StreamRoute::Motion;
        }

        let is_auxiliary = [
            "quality",
            "metric",
            "bandpower",
            "mental",
            "facial",
            "marker",
            "command",
            "devicequality",
        ]
        .iter()
        .any(|token| combined.contains(token));
        if is_auxiliary {
            return StreamRoute::Auxiliary;
        }

        let is_eeg = combined.contains("eeg");
        if is_eeg && channel_count >= 2 {
            return StreamRoute::Eeg;
        }

        StreamRoute::Unknown
    }

    fn route_allows_feature_extraction(route: StreamRoute) -> bool {
        matches!(route, StreamRoute::Eeg)
    }

    async fn publish_stream_route_counts(&self) {
        let mut eeg = 0u64;
        let mut motion = 0u64;
        let mut auxiliary = 0u64;
        let mut unknown = 0u64;

        for route in self.stream_routes.values() {
            match route {
                StreamRoute::Eeg => eeg += 1,
                StreamRoute::Motion => motion += 1,
                StreamRoute::Auxiliary => auxiliary += 1,
                StreamRoute::Unknown => unknown += 1,
            }
        }

        let mut state = self.state.write().await;
        state.routed_eeg_streams = eeg;
        state.routed_motion_streams = motion;
        state.routed_auxiliary_streams = auxiliary;
        state.routed_unknown_streams = unknown;
    }

    fn rebuild_stream_pipelines(&mut self) {
        for stream in self.stream_buffers.values_mut() {
            stream.rebuild_pipeline(&self.config);
        }
    }

    async fn publish_stream_runtime_metrics(
        &self,
        stream_key: &str,
        metrics: &StreamRuntimeMetrics,
    ) {
        let total_integrity_issues = self.stream_buffers.values().fold(0u64, |total, stream| {
            total.saturating_add(stream.integrity_issues)
        });

        let degraded_eeg_streams = self
            .stream_buffers
            .iter()
            .filter(|(id, stream)| {
                self.stream_routes.get(*id).copied() == Some(StreamRoute::Eeg)
                    && stream.integrity_state() != "ok"
            })
            .count() as u64;
        let total_eeg_streams = self
            .stream_buffers
            .keys()
            .filter(|id| self.stream_routes.get(*id).copied() == Some(StreamRoute::Eeg))
            .count() as u64;

        let mut state = self.state.write().await;
        if let Some(discovered) = state.discovered_streams.iter_mut().find(|stream| {
            stream.id == stream_key || stream.source_id.as_deref() == Some(stream_key)
        }) {
            discovered.effective_sample_rate_hz = metrics.effective_sample_rate_hz;
            discovered.samples_received = metrics.samples_received;
            discovered.samples_dropped = metrics.samples_dropped;
            discovered.drop_rate_pct = metrics.drop_rate_pct;
            discovered.last_sample_age_ms = metrics.last_sample_age_ms;
            discovered.preprocessing_summary = metrics.preprocessing_summary.clone();
            discovered.integrity_state = metrics.integrity_state.clone();
        }

        state.set_signal_integrity_snapshot(
            total_integrity_issues,
            total_eeg_streams,
            degraded_eeg_streams,
        );
    }

    /// Runs the signal task until shutdown is signaled.
    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        tracing::info!(
            event = obs::event::TASK_STARTED,
            span = obs::span::SIGNAL_RUN,
            stage = obs::stage::SIGNAL,
            feature_window_ms = self.config.feature_window_ms,
            feature_step_ms = self.config.feature_step_ms,
            buffer_size_samples = self.config.buffer_size_samples,
            "Signal processing task started"
        );
        {
            let mut state = self.state.write().await;
            state.set_stage_integrity_snapshot(IntegrityStage::Signal, 0, false);
        }

        loop {
            // Apply all pending runtime config updates without blocking the stream.
            let mut pending_config: Option<SignalConfig> = None;
            if let Some(rx) = &mut self.signal_command_rx {
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        SignalCommand::UpdateConfig(cfg) => {
                            pending_config = Some(cfg);
                        }
                    }
                }
            }
            if let Some(cfg) = pending_config {
                self.config = cfg;
                self.rebuild_stream_pipelines();
                tracing::info!("SignalTask config updated at runtime");
            }

            tokio::select! {
                // Check for shutdown
                _ = shutdown.recv() => {
                    tracing::info!(event = obs::event::TASK_STOPPED, "Signal task received shutdown signal");
                    break;
                }

                // Receive samples from device task (any stream)
                sample = self.sample_rx.recv() => {
                    match sample {
                        Some(sample) => {
                            self.sample_count += 1;

                            if let Some(tx) = &self.sample_tap_tx {
                                let _ = tx.try_send(sample.clone());
                            }

                            // Broadcast raw sample to hub visualization widgets
                            if let Some(tx) = &self.sample_broadcast_tx {
                                let _ = tx.send(sample.clone());
                            }

                            self.maybe_emit_head_movement_marker(&sample);

                            // Route sample to the correct per-stream buffer
                            let stream_key = sample.source_id.clone()
                                .unwrap_or_else(|| DEFAULT_STREAM_KEY.to_string());

                            let discovered_stream = self
                                .state
                                .try_read()
                                .ok()
                                .and_then(|state| {
                                    state
                                        .discovered_streams
                                        .iter()
                                        .find(|stream| {
                                            stream.id == stream_key
                                                || stream.source_id.as_deref() == Some(&stream_key)
                                        })
                                        .cloned()
                                });
                            let route = Self::classify_stream_route(
                                &stream_key,
                                discovered_stream.as_ref(),
                                &sample,
                            );
                            if self.stream_routes.get(&stream_key).copied() != Some(route) {
                                self.stream_routes.insert(stream_key.clone(), route);
                                tracing::info!(
                                    stream_id = %stream_key,
                                    route = ?route,
                                    stream_type = discovered_stream
                                        .as_ref()
                                        .map(|s| s.stream_type.as_str())
                                        .unwrap_or("unknown"),
                                    channel_count = discovered_stream
                                        .as_ref()
                                        .map(|s| s.channel_count)
                                        .unwrap_or(sample.values.len() as i32),
                                    "Signal routing classified stream"
                                );
                                self.publish_stream_route_counts().await;
                            }

                            let nominal_sample_rate_hz = discovered_stream
                                .as_ref()
                                .map(|stream| stream.sample_rate as f32)
                                .filter(|rate| rate.is_finite() && *rate > 0.0)
                                .unwrap_or(128.0);
                            let sample_timestamp =
                                sample.device_timestamp.unwrap_or(sample.system_timestamp);

                            let (maybe_features, runtime_metrics) = {
                                let buffer = self.stream_buffers.entry(stream_key.clone()).or_insert_with(|| {
                                    StreamBuffer::new(
                                        &self.config,
                                        sample.values.len(),
                                        nominal_sample_rate_hz,
                                    )
                                });

                                let sample_channel_count = sample.values.len().max(1);
                                if buffer.channel_count != sample_channel_count {
                                    let expected_channel_count = buffer.channel_count;
                                    buffer.channel_count = sample_channel_count;
                                    buffer.rebuild_pipeline(&self.config);
                                    buffer.integrity_issues =
                                        buffer.integrity_issues.saturating_add(1);
                                    tracing::warn!(
                                        event = obs::event::INTEGRITY_ISSUE,
                                        stage = obs::stage::SIGNAL,
                                        decision_id = obs::field::UNKNOWN,
                                        stream_id = %stream_key,
                                        issue = "channel_count_changed",
                                        expected = expected_channel_count,
                                        got = sample_channel_count,
                                        "Signal stage channel-count mismatch detected"
                                    );
                                }

                                if let Some((previous_us, current_us)) =
                                    buffer.update_sample_rate_estimate(&sample)
                                {
                                    tracing::warn!(
                                        event = obs::event::INTEGRITY_ISSUE,
                                        stage = obs::stage::SIGNAL,
                                        decision_id = obs::field::UNKNOWN,
                                        stream_id = %stream_key,
                                        issue = "timestamp_regression",
                                        previous_us,
                                        current_us,
                                        "Signal stage timestamp regression detected"
                                    );
                                }
                                if let Some(sequence_issue) =
                                    buffer.record_sequence(sample.sequence_number)
                                {
                                    match sequence_issue {
                                        SignalSequenceIssue::Gap {
                                            previous,
                                            current,
                                            missing,
                                        } => {
                                            tracing::warn!(
                                                event = obs::event::INTEGRITY_ISSUE,
                                                stage = obs::stage::SIGNAL,
                                                decision_id = obs::field::UNKNOWN,
                                                stream_id = %stream_key,
                                                issue = "sequence_gap",
                                                previous,
                                                current,
                                                missing,
                                                "Signal stage sequence gap detected"
                                            );
                                        }
                                        SignalSequenceIssue::Regression { previous, current } => {
                                            tracing::warn!(
                                                event = obs::event::INTEGRITY_ISSUE,
                                                stage = obs::stage::SIGNAL,
                                                decision_id = obs::field::UNKNOWN,
                                                stream_id = %stream_key,
                                                issue = "sequence_regression",
                                                previous,
                                                current,
                                                "Signal stage sequence regression detected"
                                            );
                                        }
                                    }
                                }
                                buffer.samples_received = buffer.samples_received.saturating_add(1);
                                buffer.last_quality = sample.quality.clone();

                                let mut maybe_features = None;
                                if Self::route_allows_feature_extraction(route) {
                                    if let Some(pipeline) = &mut buffer.pipeline {
                                        match pipeline.push_sample(&sample.values, sample_timestamp) {
                                            Ok(()) => match pipeline.try_extract() {
                                                Ok(Some(mut features)) => {
                                                    features.stream_id = (stream_key != DEFAULT_STREAM_KEY)
                                                        .then(|| stream_key.clone());
                                                    features.window_end_us = Some(features.timestamp);
                                                    let window_us =
                                                        i64::from(self.config.feature_window_ms.max(1))
                                                            .saturating_mul(1000);
                                                    features.window_start_us = Some(
                                                        features.timestamp.saturating_sub(window_us),
                                                    );
                                                    maybe_features = Some(features);
                                                }
                                                Ok(None) => {}
                                                Err(error) => {
                                                    buffer.integrity_issues = buffer
                                                        .integrity_issues
                                                        .saturating_add(1);
                                                    tracing::warn!(
                                                        event = obs::event::INTEGRITY_ISSUE,
                                                        stage = obs::stage::SIGNAL,
                                                        decision_id = obs::field::UNKNOWN,
                                                        stream_id = %stream_key,
                                                        issue = "feature_extraction_failed",
                                                        "Signal feature extraction failed: {}",
                                                        error
                                                    );
                                                }
                                            },
                                            Err(error) => {
                                                buffer.integrity_issues =
                                                    buffer.integrity_issues.saturating_add(1);
                                                tracing::warn!(
                                                    event = obs::event::INTEGRITY_ISSUE,
                                                    stage = obs::stage::SIGNAL,
                                                    decision_id = obs::field::UNKNOWN,
                                                    stream_id = %stream_key,
                                                    issue = "pipeline_rejected_sample",
                                                    "Signal pipeline rejected sample: {}",
                                                    error
                                                );
                                            }
                                        }
                                    } else {
                                        buffer.integrity_issues =
                                            buffer.integrity_issues.saturating_add(1);
                                        tracing::warn!(
                                            event = obs::event::INTEGRITY_ISSUE,
                                            stage = obs::stage::SIGNAL,
                                            decision_id = obs::field::UNKNOWN,
                                            stream_id = %stream_key,
                                            issue = "pipeline_unavailable",
                                            "Signal pipeline unavailable for stream"
                                        );
                                    }
                                }

                                let now_micros = neurohid_types::now_micros();
                                let runtime_metrics = StreamRuntimeMetrics {
                                    effective_sample_rate_hz: Some(buffer.estimated_sample_rate_hz as f64),
                                    samples_received: Some(buffer.samples_received),
                                    samples_dropped: Some(buffer.samples_dropped),
                                    drop_rate_pct: buffer.drop_rate_pct(),
                                    last_sample_age_ms: buffer
                                        .last_sample_timestamp_micros
                                        .map(|ts| now_micros.saturating_sub(ts) as u64 / 1000),
                                    preprocessing_summary: Some(buffer.preprocessing_summary.clone()),
                                    integrity_state: Some(buffer.integrity_state().to_string()),
                                };

                                (maybe_features, runtime_metrics)
                            };

                            self.record_signal_latency(sample_timestamp).await;
                            self.publish_stream_runtime_metrics(&stream_key, &runtime_metrics)
                                .await;

                            if let Some(features) = maybe_features {
                                if let Some(tx) = &self.feature_broadcast_tx {
                                    let _ = tx.send(features.clone());
                                }

                                if tracing::enabled!(tracing::Level::DEBUG)
                                    && self.sample_count.is_multiple_of(SIGNAL_FEATURE_DEBUG_EVERY_SAMPLES)
                                    && self.emit_gate.allow_debug()
                                {
                                    tracing::debug!(
                                        event = obs::event::FEATURE_WINDOW_EMITTED,
                                        decision_id = obs::field::UNKNOWN,
                                        stream_id = features
                                            .stream_id
                                            .as_deref()
                                            .unwrap_or(DEFAULT_STREAM_KEY),
                                        feature_dim = features.dim(),
                                        window_start_us = features.window_start_us.unwrap_or_default(),
                                        window_end_us = features.window_end_us.unwrap_or_default(),
                                        "Signal feature window emitted"
                                    );
                                }

                                if self.feature_tx.send(features).await.is_err() {
                                    tracing::warn!(stream_id = %stream_key, "Feature receiver dropped");
                                    break;
                                }
                            } else if !Self::route_allows_feature_extraction(route)
                                && self.sample_count.is_multiple_of(SIGNAL_FEATURE_DEBUG_EVERY_SAMPLES)
                                && self.emit_gate.allow_info()
                            {
                                tracing::info!(
                                    stream_id = %stream_key,
                                    route = ?route,
                                    "Skipping decoder feature extraction for non-EEG stream"
                                );
                            }

                            if self.sample_count.is_multiple_of(SIGNAL_SUMMARY_EVERY_SAMPLES)
                                && self.emit_gate.allow_info()
                            {
                                tracing::info!(
                                    event = obs::event::TASK_SUMMARY,
                                    decision_id = obs::field::UNKNOWN,
                                    stream_id = obs::field::UNKNOWN,
                                    sample_count = self.sample_count,
                                    active_streams = self.stream_buffers.len(),
                                    "Signal task periodic summary"
                                );
                            }

                            // Update aggregate signal quality across all streams
                            self.update_signal_quality().await;
                        }
                        None => {
                            // Sample sender dropped
                            tracing::info!("Sample channel closed");
                            break;
                        }
                    }
                }

                // Receive ErrP results (for coordinating online learning)
                errp = self.errp_rx.recv() => {
                    if let Some(result) = errp {
                        let mut state = self.state.write().await;
                        let success = (1.0 - result.error_probability).clamp(0.0, 1.0);
                        state.rolling_success_score =
                            state.rolling_success_score * 0.9 + success * 0.1;
                        if result.error_probability > 0.5 {
                            state.errors_detected += 1;
                        }
                    }
                }
            }
        }

        tracing::info!(
            event = obs::event::TASK_STOPPED,
            decision_id = obs::field::UNKNOWN,
            stream_id = obs::field::UNKNOWN,
            sample_count = self.sample_count,
            "Signal task processed samples"
        );
        Ok(())
    }

    /// Update the aggregate signal quality across all active stream buffers.
    async fn update_signal_quality(&self) {
        // Compute per-stream quality from the most recent quality vectors, then average.
        let mut total_quality = 0.0f32;
        let mut stream_count = 0u32;

        for buf in self.stream_buffers.values() {
            if let Some(quality) = &buf.last_quality
                && !quality.is_empty()
            {
                let avg = quality.iter().sum::<f32>() / quality.len() as f32;
                total_quality += avg;
                stream_count += 1;
            }
        }

        if stream_count > 0 {
            let mut state = self.state.write().await;
            state.signal_quality = total_quality / stream_count as f32;
        }
    }

    async fn record_signal_latency(&mut self, sample_timestamp: i64) {
        if sample_timestamp <= 0 {
            return;
        }

        let now_micros = neurohid_types::now_micros();
        let latency_us = now_micros.saturating_sub(sample_timestamp) as u64;
        self.signal_latency.record(latency_us);

        let mut state = self.state.write().await;
        state.signal_latency_last_us = self.signal_latency.last_us();
        state.signal_latency_p95_us = self.signal_latency.p95_us();
    }

    fn maybe_emit_head_movement_marker(&mut self, sample: &Sample) {
        let Some(tx) = &self.marker_broadcast_tx else {
            return;
        };

        let source_id = sample.source_id.as_deref().unwrap_or_default();
        let lower = source_id.to_ascii_lowercase();
        let motion_like =
            lower.contains("motion") || lower.contains("acc") || lower.contains("imu");
        if !motion_like || sample.values.len() < 3 {
            return;
        }

        let x = sample.values[0];
        let y = sample.values[1];
        let z = sample.values[2];
        let magnitude = (x * x + y * y + z * z).sqrt();
        if magnitude < self.head_movement_threshold {
            return;
        }

        let ts = sample.device_timestamp.unwrap_or(sample.system_timestamp);
        if ts.saturating_sub(self.last_head_movement_marker_us) < 250_000 {
            return;
        }
        self.last_head_movement_marker_us = ts;

        let mut marker = StreamMarker::now(MarkerType::HeadMovement)
            .with_payload(MarkerPayload::HeadMovement { magnitude });
        marker.timestamp = ts;
        if let Some(source_id) = &sample.source_id {
            marker = marker.with_source_id(source_id.clone());
        }
        let _ = tx.send(marker);
    }
}

#[async_trait]
impl SignalPreprocessor for SignalTask {
    async fn run(self: Box<Self>, shutdown: tokio::sync::broadcast::Receiver<()>) -> Result<()> {
        (*self).run(shutdown).await
    }
}

/// Builds either the built-in SignalTask or a loaded signal preprocessing extension.
/// Returns the runner and its display name for snapshot ("built-in" or extension name).
#[allow(clippy::too_many_arguments)]
pub fn create_signal_preprocessor(
    config: SignalConfig,
    sample_rx: mpsc::Receiver<Sample>,
    feature_tx: mpsc::Sender<FeatureVector>,
    errp_rx: mpsc::Receiver<ErrPResult>,
    state: Arc<RwLock<ServiceState>>,
    signal_command_rx: Option<mpsc::Receiver<SignalCommand>>,
    sample_tap_tx: Option<mpsc::Sender<Sample>>,
    sample_broadcast_tx: Option<broadcast::Sender<Sample>>,
    feature_broadcast_tx: Option<broadcast::Sender<FeatureVector>>,
    marker_broadcast_tx: Option<broadcast::Sender<StreamMarker>>,
    observability: ObservabilityConfig,
    registry: Option<&crate::extension_registry::ExtensionRegistry>,
) -> Result<(Box<dyn SignalPreprocessor + Send + Sync>, String)> {
    if let Some(ref ext_name) = config.extension_name {
        let name = ext_name.clone();
        let reg = registry.ok_or_else(|| neurohid_types::error::ExtensionError::LoadError {
            name: name.clone(),
            reason: "extension registry not available (signal extension requires registry)"
                .to_string(),
        })?;
        let channels = SignalChannels {
            sample_rx,
            feature_tx,
        };
        let loaded = reg.load_signal_preprocessor(&name, config, channels)?;
        return Ok((Box::new(loaded), name));
    }
    let task = SignalTask::new(
        config,
        sample_rx,
        feature_tx,
        errp_rx,
        state,
        signal_command_rx,
        sample_tap_tx,
        sample_broadcast_tx,
        feature_broadcast_tx,
        marker_broadcast_tx,
        observability,
    );
    Ok((Box::new(task), "built-in".to_string()))
}

#[cfg(test)]
mod tests {
    use super::config;
    use super::SignalTask;
    use super::StreamRoute;
    use neurohid_types::device::DiscoveredStream;
    use neurohid_types::signal::Sample;

    #[test]
    fn samples_for_duration_uses_expected_rate() {
        assert_eq!(config::samples_for_duration_ms(500, 128.0), 64);
        assert_eq!(config::samples_for_duration_ms(50, 128.0), 6);
    }

    #[test]
    fn samples_for_duration_clamps_invalid_rate() {
        assert_eq!(config::samples_for_duration_ms(500, f32::NAN), 64);
        assert_eq!(config::samples_for_duration_ms(500, 0.0), 4);
    }

    #[test]
    fn samples_for_duration_never_returns_zero() {
        assert_eq!(config::samples_for_duration_ms(0, 256.0), 1);
        assert_eq!(config::samples_for_duration_ms(1, 8.0), 1);
    }

    #[test]
    fn welch_segment_len_is_power_of_two_within_window() {
        assert_eq!(config::welch_segment_len_for_window(1), 1);
        assert_eq!(config::welch_segment_len_for_window(2), 2);
        assert_eq!(config::welch_segment_len_for_window(63), 32);
        assert_eq!(config::welch_segment_len_for_window(64), 64);
        assert_eq!(config::welch_segment_len_for_window(200), 128);
        assert_eq!(config::welch_segment_len_for_window(1024), 256);
    }

    #[test]
    fn classify_stream_route_maps_emotiv_eeg() {
        let discovered = DiscoveredStream {
            id: "src::EmotivEEG".to_string(),
            name: "EmotivEEG".to_string(),
            stream_type: "EEG".to_string(),
            channel_count: 5,
            sample_rate: 128.0,
            connected: true,
            battery_percent: None,
            channel_quality: None,
            source_id: Some("src".to_string()),
            effective_sample_rate_hz: None,
            samples_received: None,
            samples_dropped: None,
            drop_rate_pct: None,
            last_sample_age_ms: None,
            preprocessing_summary: None,
            integrity_state: None,
        };
        let sample = Sample::new(vec![0.0; 5]);

        let route = SignalTask::classify_stream_route("src::EmotivEEG", Some(&discovered), &sample);
        assert_eq!(route, StreamRoute::Eeg);
    }

    #[test]
    fn classify_stream_route_maps_emotiv_mental_to_auxiliary() {
        let discovered = DiscoveredStream {
            id: "src::EmotivMentalCommands".to_string(),
            name: "EmotivMentalCommands".to_string(),
            stream_type: "MentalCommand".to_string(),
            channel_count: 1,
            sample_rate: 0.0,
            connected: true,
            battery_percent: None,
            channel_quality: None,
            source_id: Some("src".to_string()),
            effective_sample_rate_hz: None,
            samples_received: None,
            samples_dropped: None,
            drop_rate_pct: None,
            last_sample_age_ms: None,
            preprocessing_summary: None,
            integrity_state: None,
        };
        let sample = Sample::new(vec![0.5]);

        let route = SignalTask::classify_stream_route(
            "src::EmotivMentalCommands",
            Some(&discovered),
            &sample,
        );
        assert_eq!(route, StreamRoute::Auxiliary);
    }
}
