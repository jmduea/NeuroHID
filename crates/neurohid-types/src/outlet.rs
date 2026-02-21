//! # Outlet and extension contracts
//!
//! Defines the outlet/effector contract and extension manifest so that
//! built-in and extension implementations can be swapped by the runtime.
//!
//! Extension identity is by **name only** (no version in ID). Duplicate
//! extension names across discovered manifests are an error.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::error::Result;
use crate::event::StreamMarker;
use crate::signal::{FeatureVector, Sample};
use crate::action::Action;

/// Slot kind for pipeline extensions. Used by the registry to list extensions per slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionKind {
    /// Output/effector: consumes sample, feature, action, marker streams.
    Outlet,
    /// Acquisition: device discovery and sample stream.
    Device,
    /// Signal preprocessing: raw samples → feature vectors.
    SignalPreprocessing,
    /// Decoder: feature vectors → actions.
    Decoder,
}

/// Extension manifest (e.g. in `manifest.json`). Name is the sole ID; duplicate names
/// across discovered extensions cause discovery to fail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionManifest {
    /// Extension name — the sole identifier. No version in ID.
    pub name: String,
    /// Slot this extension implements.
    pub kind: ExtensionKind,
}

/// Channel handles passed into an outlet runner (same shape as the outlet task in neurohid-core).
pub struct OutletChannels {
    pub sample_rx: Option<broadcast::Receiver<Sample>>,
    pub feature_rx: Option<broadcast::Receiver<FeatureVector>>,
    pub action_rx: Option<broadcast::Receiver<Action>>,
    pub marker_rx: Option<broadcast::Receiver<StreamMarker>>,
}

/// Outlet/effector contract: receives config and four broadcast receivers, runs until shutdown.
///
/// Implementations can be built-in (e.g. LSL/TCP in neurohid-core) or loaded extensions.
/// The runtime constructs the implementor with config and channels, then calls `run` once.
#[async_trait]
pub trait Outlet: Send + Sync {
    /// Run until shutdown is signalled. Consumes self (use `Box<Self>` for trait objects).
    async fn run(
        self: Box<Self>,
        shutdown: broadcast::Receiver<()>,
    ) -> Result<()>;
}
