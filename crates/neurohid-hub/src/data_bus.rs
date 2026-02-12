//! # Data Bus
//!
//! Fan-out data distribution from the running service to N widget subscribers.
//! The bus collects samples, features, and actions from the service's broadcast
//! channels and maintains ring-buffer snapshots that widgets read each frame.

use std::collections::{HashMap, VecDeque};
use tokio::sync::broadcast;

use neurohid_types::{
    action::Action,
    device::DiscoveredStream,
    event::StreamMarker,
    signal::{FeatureVector, Sample},
};

/// Maximum number of raw samples to keep in the ring buffer (≈10s at 128 Hz).
const MAX_SAMPLES: usize = 1280;
/// Maximum number of feature vectors to keep.
const MAX_FEATURES: usize = 200;
/// Maximum number of actions to keep.
const MAX_ACTIONS: usize = 200;
/// Maximum number of timeline markers to keep.
const MAX_MARKERS: usize = 512;

/// The data bus collects live data from service broadcast channels and
/// maintains ring-buffer snapshots that widgets can read each frame.
pub struct DataBus {
    sample_rx: Option<broadcast::Receiver<Sample>>,
    feature_rx: Option<broadcast::Receiver<FeatureVector>>,
    action_rx: Option<broadcast::Receiver<Action>>,
    marker_rx: Option<broadcast::Receiver<StreamMarker>>,

    /// Ring buffer of recent raw samples (all streams, for backward compat).
    pub samples: VecDeque<Sample>,
    /// Per-source ring buffers keyed by `Sample::source_id`.
    /// Samples without a `source_id` are stored under the empty-string key.
    pub samples_by_source: HashMap<String, VecDeque<Sample>>,
    /// Ring buffer of recent feature vectors.
    pub features: VecDeque<FeatureVector>,
    /// Ring buffer of recent decoded actions.
    pub actions: VecDeque<Action>,
    /// Ring buffer of recent marker annotations.
    pub markers: VecDeque<StreamMarker>,

    /// Monotonically increasing counter of total samples received.
    /// Unlike `samples.len()`, this never saturates at MAX_SAMPLES.
    pub total_samples_received: u64,
}

impl DataBus {
    /// Create a new, disconnected data bus.
    pub fn new() -> Self {
        Self {
            sample_rx: None,
            feature_rx: None,
            action_rx: None,
            marker_rx: None,
            samples: VecDeque::with_capacity(MAX_SAMPLES),
            samples_by_source: HashMap::new(),
            features: VecDeque::with_capacity(MAX_FEATURES),
            actions: VecDeque::with_capacity(MAX_ACTIONS),
            markers: VecDeque::with_capacity(MAX_MARKERS),
            total_samples_received: 0,
        }
    }

    /// Connect the bus to the service's broadcast channels.
    /// Called whenever the service (re)starts.
    pub fn connect(
        &mut self,
        sample_rx: broadcast::Receiver<Sample>,
        feature_rx: broadcast::Receiver<FeatureVector>,
        action_rx: broadcast::Receiver<Action>,
        marker_rx: broadcast::Receiver<StreamMarker>,
    ) {
        self.sample_rx = Some(sample_rx);
        self.feature_rx = Some(feature_rx);
        self.action_rx = Some(action_rx);
        self.marker_rx = Some(marker_rx);
    }

    /// Disconnect and clear receivers (called when service stops).
    pub fn disconnect(&mut self) {
        self.sample_rx = None;
        self.feature_rx = None;
        self.action_rx = None;
        self.marker_rx = None;
    }

    /// Drain all pending messages from broadcast channels into ring buffers.
    /// Called once per frame from the GUI thread (non-blocking).
    pub fn poll(&mut self) {
        // Drain samples
        if let Some(rx) = &mut self.sample_rx {
            loop {
                match rx.try_recv() {
                    Ok(sample) => {
                        // Route to per-source buffer.
                        let key = sample.source_id.clone().unwrap_or_default();
                        let per_source = self
                            .samples_by_source
                            .entry(key)
                            .or_insert_with(|| VecDeque::with_capacity(MAX_SAMPLES));
                        if per_source.len() >= MAX_SAMPLES {
                            per_source.pop_front();
                        }
                        per_source.push_back(sample.clone());

                        // Also keep the flat buffer for backward compat.
                        if self.samples.len() >= MAX_SAMPLES {
                            self.samples.pop_front();
                        }
                        self.samples.push_back(sample);
                        self.total_samples_received += 1;
                        // Log receipt of the very first sample for diagnostics.
                        if self.total_samples_received == 1 {
                            tracing::info!("DataBus: first sample received");
                        }
                    }
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        tracing::trace!("Sample bus lagged by {} messages", n);
                        // Continue draining — the receiver auto-advances past lag
                    }
                    Err(_) => break,
                }
            }
        }

        // Drain features
        if let Some(rx) = &mut self.feature_rx {
            loop {
                match rx.try_recv() {
                    Ok(feature) => {
                        if self.features.len() >= MAX_FEATURES {
                            self.features.pop_front();
                        }
                        self.features.push_back(feature);
                    }
                    Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        }

        // Drain actions
        if let Some(rx) = &mut self.action_rx {
            loop {
                match rx.try_recv() {
                    Ok(action) => {
                        if self.actions.len() >= MAX_ACTIONS {
                            self.actions.pop_front();
                        }
                        self.actions.push_back(action);
                    }
                    Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        }

        // Drain markers
        if let Some(rx) = &mut self.marker_rx {
            loop {
                match rx.try_recv() {
                    Ok(marker) => {
                        if self.markers.len() >= MAX_MARKERS {
                            self.markers.pop_front();
                        }
                        self.markers.push_back(marker);
                    }
                    Err(broadcast::error::TryRecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }
        }
    }

    /// Whether any broadcast receiver is connected.
    pub fn is_connected(&self) -> bool {
        self.sample_rx.is_some()
    }

    /// Get samples belonging to streams whose `stream_type` matches one of
    /// the given types. Uses the `DiscoveredStream` list to resolve
    /// `source_id` → `stream_type`.
    ///
    /// Returns a reference to the per-source ring buffer if exactly one
    /// matching stream is found, which is the common case and avoids
    /// any allocation. When multiple streams of the same type exist
    /// (rare), the first match is returned.
    ///
    /// Falls back to the flat `samples` buffer when:
    ///   - No discovered streams are available (pre-connection).
    ///   - No per-source buffers have been populated yet.
    pub fn samples_for_type<'a>(
        &'a self,
        stream_types: &[&str],
        streams: &[DiscoveredStream],
    ) -> &'a VecDeque<Sample> {
        // Find the first DiscoveredStream whose stream_type matches and
        // that has data in the per-source map.
        // Stream types may be composite strings like "EEG/EmotivEEG" so we
        // match against the prefix before '/' (the LSL stream type).
        for ds in streams {
            let ds_type_prefix = ds.stream_type.split('/').next().unwrap_or("");
            if stream_types
                .iter()
                .any(|st| ds_type_prefix.eq_ignore_ascii_case(st))
            {
                if let Some(buf) = self.samples_by_source.get(&ds.id) {
                    if !buf.is_empty() {
                        return buf;
                    }
                }
            }
        }
        // Fallback: return the flat buffer (backward compat / single-stream).
        &self.samples
    }
}
