//! # Data Bus
//!
//! Fan-out data distribution from the running service to N widget subscribers.
//! The bus collects samples, features, and actions from the service's broadcast
//! channels and maintains ring-buffer snapshots that widgets read each frame.

use std::collections::VecDeque;
use tokio::sync::broadcast;

use neurohid_types::{
    action::Action,
    signal::{FeatureVector, Sample},
};

/// Maximum number of raw samples to keep in the ring buffer (≈10s at 128 Hz).
const MAX_SAMPLES: usize = 1280;
/// Maximum number of feature vectors to keep.
const MAX_FEATURES: usize = 200;
/// Maximum number of actions to keep.
const MAX_ACTIONS: usize = 200;

/// The data bus collects live data from service broadcast channels and
/// maintains ring-buffer snapshots that widgets can read each frame.
pub struct DataBus {
    sample_rx: Option<broadcast::Receiver<Sample>>,
    feature_rx: Option<broadcast::Receiver<FeatureVector>>,
    action_rx: Option<broadcast::Receiver<Action>>,

    /// Ring buffer of recent raw samples.
    pub samples: VecDeque<Sample>,
    /// Ring buffer of recent feature vectors.
    pub features: VecDeque<FeatureVector>,
    /// Ring buffer of recent decoded actions.
    pub actions: VecDeque<Action>,
}

impl DataBus {
    /// Create a new, disconnected data bus.
    pub fn new() -> Self {
        Self {
            sample_rx: None,
            feature_rx: None,
            action_rx: None,
            samples: VecDeque::with_capacity(MAX_SAMPLES),
            features: VecDeque::with_capacity(MAX_FEATURES),
            actions: VecDeque::with_capacity(MAX_ACTIONS),
        }
    }

    /// Connect the bus to the service's broadcast channels.
    /// Called whenever the service (re)starts.
    pub fn connect(
        &mut self,
        sample_rx: broadcast::Receiver<Sample>,
        feature_rx: broadcast::Receiver<FeatureVector>,
        action_rx: broadcast::Receiver<Action>,
    ) {
        self.sample_rx = Some(sample_rx);
        self.feature_rx = Some(feature_rx);
        self.action_rx = Some(action_rx);
    }

    /// Disconnect and clear receivers (called when service stops).
    pub fn disconnect(&mut self) {
        self.sample_rx = None;
        self.feature_rx = None;
        self.action_rx = None;
    }

    /// Drain all pending messages from broadcast channels into ring buffers.
    /// Called once per frame from the GUI thread (non-blocking).
    pub fn poll(&mut self) {
        // Drain samples
        if let Some(rx) = &mut self.sample_rx {
            loop {
                match rx.try_recv() {
                    Ok(sample) => {
                        if self.samples.len() >= MAX_SAMPLES {
                            self.samples.pop_front();
                        }
                        self.samples.push_back(sample);
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
    }

    /// Whether any broadcast receiver is connected.
    pub fn is_connected(&self) -> bool {
        self.sample_rx.is_some()
    }
}
