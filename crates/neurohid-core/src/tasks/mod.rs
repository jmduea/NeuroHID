//! # Service Tasks
//!
//! This module contains the individual tasks that run concurrently within the
//! NeuroHID service. Each task has a single responsibility and communicates
//! with other tasks through channels.
//!
//! Think of each task as a worker on an assembly line: each one has a specific
//! job, receives input from the worker before them, does their work, and passes
//! the result to the next worker. This design keeps each piece simple and makes
//! it easy to test them independently.

mod action;
mod decoder;
mod device;
mod ipc;
mod latency;
mod latency_alert;
mod outlet;
mod session_logger;
mod signal;

use neurohid_types::IpcEnvelopeV3;
use neurohid_types::{Timestamp, action::Action};
use tokio::sync::mpsc;

/// Decoder-emitted event forwarded to the runtime ML bridge.
#[derive(Debug, Clone)]
pub struct DecisionEventRecord {
    pub decision_id: String,
    pub timestamp_us: Timestamp,
    pub feature_values: Vec<f32>,
    pub action: Action,
    pub decoder_confidence: f32,
    pub signal_quality: f32,
    pub decoder_model_version: Option<String>,
    pub stream_id: Option<String>,
}

/// Trainer ingress events injected by the unified IPC server.
#[derive(Debug, Clone)]
pub enum TrainerIngressEvent {
    Connected { session_id: String },
    Envelope(IpcEnvelopeV3),
    Disconnected,
}

/// In-process channels connecting service IPC transport and trainer protocol engine.
pub struct TrainerBridgeChannels {
    pub ingress_tx: mpsc::Sender<TrainerIngressEvent>,
    pub egress_rx: mpsc::Receiver<IpcEnvelopeV3>,
}

pub use action::ActionTask;
pub use decoder::DecoderTask;
pub use device::DeviceTask;
pub use ipc::IpcTask;
pub use latency_alert::LatencyAlertMonitorTask;
pub use outlet::OutletTask;
pub use session_logger::{EpisodeLogRecord, SessionLoggerTask};
pub use signal::SignalTask;
