//! LSL inlet client — pulls samples from an LSL inlet.
//!
//! Consumption model, timestamps, and "latest sample" semantics are defined in
//! `docs/formats/stream-semantics.md`. This implementation uses a **continuous pull** with
//! `pull_sample(0.2)` in a loop and forwards every sample (no drain-then-last); "latest sample"
//! here is the most recently received sample in the continuous stream.

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{mpsc, watch};

use neurohid_types::{
    device::{ConnectionState, DeviceId, DeviceInfo, DeviceStatus},
    error::{DeviceError, Result},
    now_micros,
    signal::{DeviceChannelConfig, Sample},
};

use crate::traits::{Device, SampleStream};

/// Wrapper around `lsl::StreamInlet` that is `Send + Sync`.
///
/// liblsl inlets are internally thread-safe (the C library uses its own locking),
/// but the Rust bindings contain a raw pointer which is `!Send` by default.
pub(crate) struct ThreadSafeInlet(pub lsl::StreamInlet);

// SAFETY: liblsl inlets are thread-safe. The underlying C library handles
// synchronization for pull operations.
unsafe impl Send for ThreadSafeInlet {}
unsafe impl Sync for ThreadSafeInlet {}

/// An LSL stream consumer that implements the [`Device`] trait.
pub struct LslDevice {
    info: DeviceInfo,
    channel_config: DeviceChannelConfig,

    /// The LSL inlet (shared with the pull thread).
    inlet: Option<Arc<ThreadSafeInlet>>,

    // State tracking
    streaming: Arc<AtomicBool>,
    samples_received: Arc<AtomicU64>,

    // Status broadcasting
    status_tx: watch::Sender<DeviceStatus>,
    status_rx: watch::Receiver<DeviceStatus>,
}

impl LslDevice {
    pub(crate) fn new(inlet: ThreadSafeInlet, info: DeviceInfo) -> Self {
        let channel_config = info
            .channel_config
            .clone()
            .unwrap_or_else(|| DeviceChannelConfig {
                channels: Vec::new(),
                sampling_rate_hz: 0.0,
                resolution_bits: 32,
            });

        let initial_status = DeviceStatus {
            device_id: info.id.clone(),
            connection_state: ConnectionState::Connected,
            is_streaming: false,
            samples_received: 0,
            samples_dropped: 0,
            battery_percent: None,
            channel_quality: None,
            message: Some("LSL inlet opened".to_string()),
        };

        let (status_tx, status_rx) = watch::channel(initial_status);

        Self {
            info,
            channel_config,
            inlet: Some(Arc::new(inlet)),
            streaming: Arc::new(AtomicBool::new(false)),
            samples_received: Arc::new(AtomicU64::new(0)),
            status_tx,
            status_rx,
        }
    }

    fn update_status(&self) {
        let status = DeviceStatus {
            device_id: self.info.id.clone(),
            connection_state: if self.inlet.is_some() {
                ConnectionState::Connected
            } else {
                ConnectionState::Disconnected
            },
            is_streaming: self.streaming.load(Ordering::SeqCst),
            samples_received: self.samples_received.load(Ordering::SeqCst),
            samples_dropped: 0,
            battery_percent: None,
            channel_quality: None,
            message: None,
        };
        let _ = self.status_tx.send(status);
    }
}

#[async_trait]
impl Device for LslDevice {
    fn id(&self) -> &DeviceId {
        &self.info.id
    }

    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn channel_config(&self) -> &DeviceChannelConfig {
        &self.channel_config
    }

    fn status(&self) -> DeviceStatus {
        self.status_rx.borrow().clone()
    }

    fn is_connected(&self) -> bool {
        self.inlet.is_some()
    }

    fn is_streaming(&self) -> bool {
        self.streaming.load(Ordering::SeqCst)
    }

    async fn start_streaming(&mut self) -> Result<SampleStream> {
        let inlet = self.inlet.as_ref().ok_or(DeviceError::NotConnected)?;

        if self.streaming.load(Ordering::SeqCst) {
            return Err(DeviceError::DeviceBusy.into());
        }

        self.streaming.store(true, Ordering::SeqCst);
        self.update_status();

        let inlet = Arc::clone(inlet);
        let streaming = Arc::clone(&self.streaming);
        let samples_received = Arc::clone(&self.samples_received);
        let stream_id_for_thread = self.info.id.0.clone();

        let (tx, rx) = mpsc::channel::<Result<Sample>>(1024);

        // Spawn a blocking task that pulls samples from the LSL inlet.
        // liblsl's pull_sample is a blocking C call, so it must not run
        // on the Tokio async executor.
        tokio::task::spawn_blocking(move || {
            use lsl::Pullable;

            let mut sequence: u64 = 0;
            let mut consecutive_errors: u64 = 0;

            while streaming.load(Ordering::Relaxed) {
                // Pull with a short timeout so we can check the streaming flag.
                let result: std::result::Result<(Vec<f32>, f64), _> = inlet.0.pull_sample(0.2);

                match result {
                    Ok((data, timestamp)) => {
                        // timestamp == 0.0 means the pull timed out
                        if timestamp == 0.0 || data.is_empty() {
                            continue;
                        }

                        // Reset error counter on successful pull
                        if consecutive_errors > 0 {
                            tracing::info!(
                                "LSL pull recovered after {} consecutive errors",
                                consecutive_errors
                            );
                            consecutive_errors = 0;
                        }

                        sequence += 1;

                        // Log the very first sample for diagnostics
                        if sequence == 1 {
                            tracing::info!(
                                "LSL: first sample pulled from '{}' ({} channels, ts={:.3})",
                                stream_id_for_thread,
                                data.len(),
                                timestamp
                            );
                        }

                        // LSL timestamps are seconds since an arbitrary epoch.
                        // Convert to microseconds for consistency with our Sample type.
                        let device_ts = (timestamp * 1_000_000.0) as i64;
                        let system_ts = now_micros();

                        let sample = Sample {
                            source_id: Some(stream_id_for_thread.clone()),
                            device_timestamp: Some(device_ts),
                            system_timestamp: system_ts,
                            sequence_number: Some(sequence),
                            values: data,
                            quality: None,
                        };

                        samples_received.fetch_add(1, Ordering::Relaxed);

                        if tx.blocking_send(Ok(sample)).is_err() {
                            // Receiver dropped — stop pulling
                            break;
                        }
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        // Log the first error, then periodically to avoid flooding.
                        if consecutive_errors == 1 {
                            tracing::warn!(
                                "LSL pull_sample error on '{}': {:?}",
                                stream_id_for_thread,
                                e
                            );
                        } else if consecutive_errors.is_multiple_of(50) {
                            tracing::warn!(
                                "LSL pull_sample: {} consecutive errors on '{}' (latest: {:?})",
                                consecutive_errors,
                                stream_id_for_thread,
                                e
                            );
                        }
                        continue;
                    }
                }
            }

            tracing::info!(
                "LSL pull thread exiting for '{}' (pulled {} samples)",
                stream_id_for_thread,
                sequence
            );
        });

        // Convert mpsc receiver to a Stream
        let stream = futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        });

        Ok(Box::pin(stream))
    }

    async fn stop_streaming(&mut self) -> Result<()> {
        self.streaming.store(false, Ordering::SeqCst);
        self.update_status();
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.streaming.store(false, Ordering::SeqCst);
        self.inlet = None;
        self.update_status();
        Ok(())
    }

    fn status_stream(&self) -> Pin<Box<dyn Stream<Item = DeviceStatus> + Send>> {
        let rx = self.status_rx.clone();
        Box::pin(futures::stream::unfold(rx, |mut rx| async move {
            rx.changed().await.ok()?;
            let status = rx.borrow().clone();
            Some((status, rx))
        }))
    }
}

impl Drop for LslDevice {
    fn drop(&mut self) {
        // Ensure the spawn_blocking pull thread exits even if graceful
        // shutdown didn't complete (e.g., runtime dropped without awaiting
        // the device task). The thread checks this flag every 0.2s.
        self.streaming.store(false, Ordering::SeqCst);
    }
}

/// Stream-native alias for [`LslDevice`].
pub type LslInletClient = LslDevice;
