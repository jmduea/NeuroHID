//! # BrainFlow Device Backend
//!
//! Feature-gated module providing support for 30+ EEG boards through
//! the [BrainFlow](https://brainflow.org/) C library.
//!
//! Enable with: `cargo build --features brainflow`
//!
//! ## Supported Boards (validated)
//!
//! | Board                      | Channels | Sample Rate | Status         |
//! |----------------------------|----------|-------------|----------------|
//! | BrainFlow Synthetic        | 16       | 250 Hz      | ✅ Development |
//! | OpenBCI Cyton              | 8        | 250 Hz      | ✅ Tested      |
//! | OpenBCI Ganglion            | 4        | 200 Hz      | ✅ Tested      |
//! | OpenBCI Cyton+Daisy        | 16       | 125 Hz      | ⚠ Untested    |
//! | Muse 2                     | 4        | 256 Hz      | ⚠ Untested    |
//!
//! ## Architecture
//!
//! ```text
//! BrainFlow C Library
//!     ↕ FFI (brainflow-rust crate)
//! BoardShim (sync, blocking)
//!     ↕ polling thread + mpsc channel
//! BrainFlowStream (async Stream<Item = Sample>)
//!     ↕ Device trait
//! NeuroHID pipeline
//! ```
//!
//! Emotiv devices (Insight, EPOC+, EPOC X) are **not** supported through
//! BrainFlow. They use Emotiv's proprietary Cortex API — see the `emotiv` module.

pub mod board_map;
pub mod device;
pub mod provider;
pub mod stream;

pub use brainflow::BoardIds;
pub use device::BrainFlowDevice;
pub use provider::{BoardParams, BrainFlowConfig, BrainFlowDeviceProvider};
pub use stream::{BoardChannelMap, BrainFlowStream, StreamConfig};
