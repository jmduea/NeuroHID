//! # Decoder contract
//!
//! Contract for the decoder pipeline slot: consumes feature vectors, produces
//! actions (and integrates with profile/model loading if needed). Aligns with
//! how the decoder task in neurohid-core runs until shutdown.

use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc};

use crate::action::Action;
use crate::error::Result;
use crate::signal::FeatureVector;

/// Channel handles for a decoder (feature in, actions out).
pub struct DecoderChannels {
    pub feature_rx: mpsc::Receiver<FeatureVector>,
    pub action_tx: mpsc::Sender<Action>,
}

/// Decoder contract: accepts feature vectors, produces actions.
///
/// Implementations are constructed with config, profile/model context, and
/// channel handles (feature receiver, action sender); then the runtime calls
/// `run` once. Use `Box<dyn DecoderRunner>` for trait objects.
#[async_trait]
pub trait DecoderRunner: Send + Sync {
    /// Run until shutdown is signalled. Consumes self (use `Box<Self>` for trait objects).
    async fn run(
        self: Box<Self>,
        shutdown: broadcast::Receiver<()>,
    ) -> Result<()>;
}
