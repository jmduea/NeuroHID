//! # Signal preprocessing contract
//!
//! Contract for the signal preprocessing pipeline slot: consumes raw samples,
//! produces feature vectors (and optionally forwards samples/markers).
//! Aligns with how the signal task in neurohid-core runs until shutdown.

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc};

use crate::error::Result;
use crate::signal::{FeatureVector, Sample};

/// Channel handles for a signal preprocessor (minimal contract: sample in, features out).
pub struct SignalChannels {
    pub sample_rx: mpsc::Receiver<Sample>,
    pub feature_tx: mpsc::Sender<FeatureVector>,
}

/// Signal preprocessing contract: accepts raw samples, produces feature vectors.
///
/// Implementations are constructed with config and channel handles (e.g. sample
/// receiver, feature sender); then the runtime calls `run` once. Use
/// `Box<dyn SignalPreprocessor>` for trait objects.
#[async_trait]
pub trait SignalPreprocessor: Send + Sync {
    /// Run until shutdown is signalled. Consumes self (use `Box<Self>` for trait objects).
    async fn run(
        self: Box<Self>,
        shutdown: broadcast::Receiver<()>,
    ) -> Result<()>;
}
