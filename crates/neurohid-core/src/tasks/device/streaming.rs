//! Sample streaming from connected devices and integrity tracking.
//!
//! Handles spawning per-stream tasks, reading samples, forwarding to the
//! signal pipeline, and reporting integrity issues.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt;
use tokio::sync::{RwLock, mpsc};
use tokio::time::{self, Duration};
use tokio_util::sync::CancellationToken;

use neurohid_device::Device;
use neurohid_types::observability::{self as obs, EmitPolicyConfig};
use neurohid_types::signal::Sample;

use crate::observability::EmitGate;
use crate::service::{IntegrityStage, ServiceState};

/// An active stream connection managed by the device task.
pub(crate) struct ActiveStream {
    pub(crate) cancel: CancellationToken,
    pub(crate) join_handle: tokio::task::JoinHandle<()>,
}

pub(crate) struct StreamTaskContext {
    pub(crate) sample_tx: mpsc::Sender<Sample>,
    pub(crate) calibration_sample_tx: Option<mpsc::Sender<Sample>>,
    pub(crate) calibration_mode: Option<Arc<AtomicBool>>,
    pub(crate) state: Arc<RwLock<ServiceState>>,
    pub(crate) observability_policy: EmitPolicyConfig,
}

pub(super) const DEVICE_SUMMARY_EVERY_SAMPLES: u64 = 2_048;

#[derive(Debug, Clone, Copy)]
pub(super) enum DeviceIntegrityIssue {
    TimestampRegression {
        previous_us: i64,
        current_us: i64,
    },
    SequenceGap {
        previous: u64,
        current: u64,
        missing: u64,
    },
    SequenceRegression {
        previous: u64,
        current: u64,
    },
    ChannelCountChanged {
        expected: usize,
        got: usize,
    },
    StreamTaskExited,
}

pub(super) struct DeviceSampleIntegrityTracker {
    expected_channels: Option<usize>,
    last_timestamp_us: Option<i64>,
    last_sequence_number: Option<u64>,
    pub(super) samples_seen: u64,
}

impl DeviceSampleIntegrityTracker {
    pub(super) fn new() -> Self {
        Self {
            expected_channels: None,
            last_timestamp_us: None,
            last_sequence_number: None,
            samples_seen: 0,
        }
    }

    pub(super) fn observe_sample(&mut self, sample: &Sample) -> Option<DeviceIntegrityIssue> {
        self.samples_seen = self.samples_seen.saturating_add(1);

        if let Some(expected_channels) = self.expected_channels {
            let got = sample.values.len();
            if got != expected_channels {
                self.expected_channels = Some(got);
                return Some(DeviceIntegrityIssue::ChannelCountChanged {
                    expected: expected_channels,
                    got,
                });
            }
        } else {
            self.expected_channels = Some(sample.values.len());
        }

        let timestamp_us = sample.device_timestamp.unwrap_or(sample.system_timestamp);
        if let Some(previous_us) = self.last_timestamp_us
            && timestamp_us <= previous_us
        {
            self.last_timestamp_us = Some(timestamp_us);
            return Some(DeviceIntegrityIssue::TimestampRegression {
                previous_us,
                current_us: timestamp_us,
            });
        }
        self.last_timestamp_us = Some(timestamp_us);

        let sequence_number = sample.sequence_number?;
        if let Some(previous) = self.last_sequence_number {
            let expected = previous.saturating_add(1);
            if sequence_number > expected {
                let missing = sequence_number.saturating_sub(expected);
                self.last_sequence_number = Some(sequence_number);
                return Some(DeviceIntegrityIssue::SequenceGap {
                    previous,
                    current: sequence_number,
                    missing,
                });
            }
            if sequence_number <= previous {
                self.last_sequence_number = Some(sequence_number);
                return Some(DeviceIntegrityIssue::SequenceRegression {
                    previous,
                    current: sequence_number,
                });
            }
        }

        self.last_sequence_number = Some(sequence_number);
        None
    }
}

/// Spawn a tokio task that streams samples from a single connected device.
pub(super) fn spawn_stream_task(
    mut device: Box<dyn Device>,
    stream_id: String,
    context: StreamTaskContext,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let StreamTaskContext {
            sample_tx,
            calibration_sample_tx,
            calibration_mode,
            state,
            observability_policy,
        } = context;

        let mut emit_gate = EmitGate::new(observability_policy);
        let mut integrity = DeviceSampleIntegrityTracker::new();

        let stream_result = device.start_streaming().await;
        let mut sample_stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to start streaming for '{}': {}", stream_id, e);
                return;
            }
        };

        tracing::info!("Stream '{}' started", stream_id);

        let mut status_interval = time::interval(Duration::from_secs(5));
        update_stream_status(&state, &stream_id, &*device).await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("Stream '{}' cancelled", stream_id);
                    break;
                }

                sample_result = sample_stream.next() => {
                    match sample_result {
                        Some(Ok(sample)) => {
                            if let Some(issue) = integrity.observe_sample(&sample) {
                                report_device_integrity_issue(
                                    &state,
                                    &stream_id,
                                    issue,
                                    &mut emit_gate,
                                )
                                .await;
                            }

                            if let Some(quality) = &sample.quality {
                                let avg_quality =
                                    quality.iter().sum::<f32>() / quality.len() as f32;
                                let mut st = state.write().await;
                                st.signal_quality = avg_quality;
                            }

                            if let (Some(flag), Some(tx)) =
                                (&calibration_mode, &calibration_sample_tx)
                                && flag.load(Ordering::Relaxed) {
                                    let _ = tx.try_send(sample.clone());
                                }

                            if sample_tx.send(sample).await.is_err() {
                                tracing::warn!(
                                    "Sample receiver dropped, stopping stream '{}'",
                                    stream_id
                                );
                                break;
                            }

                            if integrity.samples_seen.is_multiple_of(DEVICE_SUMMARY_EVERY_SAMPLES)
                                && emit_gate.allow_info()
                            {
                                tracing::info!(
                                    event = obs::event::TASK_SUMMARY,
                                    decision_id = obs::field::UNKNOWN,
                                    stream_id = %stream_id,
                                    samples_seen = integrity.samples_seen,
                                    "Device stream periodic summary"
                                );
                            }
                        }
                        Some(Err(e)) => {
                            tracing::warn!(
                                "Error reading sample from '{}': {}",
                                stream_id,
                                e
                            );
                        }
                        None => {
                            tracing::info!("Stream '{}' ended", stream_id);
                            break;
                        }
                    }
                }

                _ = status_interval.tick() => {
                    update_stream_status(&state, &stream_id, &*device).await;
                }
            }
        }

        let _ = device.stop_streaming().await;
        let _ = device.disconnect().await;
        tracing::info!("Stream task '{}' exited", stream_id);
    })
}

/// Read the device's current status and propagate battery/quality into state.
pub(super) async fn update_stream_status(
    state: &Arc<RwLock<ServiceState>>,
    stream_id: &str,
    device: &dyn Device,
) {
    let status = device.status();
    let mut st = state.write().await;

    if let Some(ds) = st.discovered_streams.iter_mut().find(|s| s.id == stream_id) {
        ds.battery_percent = status.battery_percent;
        ds.channel_quality = status.channel_quality.clone();
    }

    st.device_battery = st
        .discovered_streams
        .iter()
        .filter(|s| s.connected)
        .find_map(|s| s.battery_percent);
}

pub(super) async fn report_device_integrity_issue(
    state: &Arc<RwLock<ServiceState>>,
    stream_id: &str,
    issue: DeviceIntegrityIssue,
    emit_gate: &mut EmitGate,
) {
    let mut st = state.write().await;
    st.register_integrity_issue(IntegrityStage::Device, true);
    if let Some(stream) = st
        .discovered_streams
        .iter_mut()
        .find(|stream| stream.id == stream_id || stream.source_id.as_deref() == Some(stream_id))
    {
        stream.integrity_state = Some("degraded".to_string());
    }
    drop(st);

    if !emit_gate.allow_info() {
        return;
    }

    match issue {
        DeviceIntegrityIssue::TimestampRegression {
            previous_us,
            current_us,
        } => {
            tracing::warn!(
                event = obs::event::INTEGRITY_ISSUE,
                stage = obs::stage::DEVICE,
                decision_id = obs::field::UNKNOWN,
                stream_id = stream_id,
                issue = "timestamp_regression",
                previous_us,
                current_us,
                "Device ingest timestamp regression detected"
            );
        }
        DeviceIntegrityIssue::SequenceGap {
            previous,
            current,
            missing,
        } => {
            tracing::warn!(
                event = obs::event::INTEGRITY_ISSUE,
                stage = obs::stage::DEVICE,
                decision_id = obs::field::UNKNOWN,
                stream_id = stream_id,
                issue = "sequence_gap",
                previous,
                current,
                missing,
                "Device ingest sequence gap detected"
            );
        }
        DeviceIntegrityIssue::SequenceRegression { previous, current } => {
            tracing::warn!(
                event = obs::event::INTEGRITY_ISSUE,
                stage = obs::stage::DEVICE,
                decision_id = obs::field::UNKNOWN,
                stream_id = stream_id,
                issue = "sequence_regression",
                previous,
                current,
                "Device ingest sequence regression detected"
            );
        }
        DeviceIntegrityIssue::ChannelCountChanged { expected, got } => {
            tracing::warn!(
                event = obs::event::INTEGRITY_ISSUE,
                stage = obs::stage::DEVICE,
                decision_id = obs::field::UNKNOWN,
                stream_id = stream_id,
                issue = "channel_count_changed",
                expected,
                got,
                "Device ingest channel-count mismatch detected"
            );
        }
        DeviceIntegrityIssue::StreamTaskExited => {
            tracing::warn!(
                event = obs::event::INTEGRITY_ISSUE,
                stage = obs::stage::DEVICE,
                decision_id = obs::field::UNKNOWN,
                stream_id = stream_id,
                issue = "stream_task_exited",
                "Device stream task exited unexpectedly"
            );
        }
    }
}
