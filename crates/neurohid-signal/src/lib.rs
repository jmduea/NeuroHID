//! # NeuroHID Signal Processing
//!
//! This crate provides the real-time signal processing pipeline for NeuroHID.
//! It takes raw samples from the device layer and transforms them into feature
//! vectors suitable for the decoder neural network.
//!
//! ## Why Signal Processing Matters
//!
//! Raw EEG signals are noisy, drifty, and hard to interpret directly. Before we
//! can use them for decoding user intent, we need to:
//!
//! 1. **Buffer samples** - Accumulate enough data for analysis (typically 0.5-2 seconds)
//! 2. **Filter** - Remove noise (powerline interference, muscle artifacts, drift)
//! 3. **Extract features** - Transform time-domain signals into informative features
//!
//! This crate handles all of this with a focus on low latency (we need features
//! within a few milliseconds of samples arriving) and real-time operation
//! (processing must keep up with the sample rate).
//!
//! ## Pipeline Architecture
//!
//! ```text
//! Raw Samples (from device)
//!        │
//!        ▼
//! ┌──────────────────┐
//! │   Ring Buffer    │  ← Thread-safe, lock-free storage
//! │   (1-8 seconds)  │
//! └────────┬─────────┘
//!          │
//!          ▼
//! ┌──────────────────┐
//! │  Filter Chain    │  ← Notch (50/60Hz), Bandpass (0.5-45Hz)
//! │                  │
//! └────────┬─────────┘
//!          │
//!          ▼
//! ┌──────────────────┐
//! │ Feature Extract  │  ← Band power, statistics, etc.
//! │                  │
//! └────────┬─────────┘
//!          │
//!          ▼
//! Feature Vector (to decoder)
//! ```
//!
//! ## Thread Safety Model
//!
//! The signal processing pipeline is designed to work in a multi-threaded context:
//!
//! - **Producer thread**: Device adapter pushes samples into the ring buffer
//! - **Consumer thread**: Main processing loop reads from buffer, filters, extracts features
//!
//! The ring buffer uses lock-free algorithms to allow concurrent push/pop without
//! blocking either thread. This is critical for maintaining low latency.
//!
//! ## Feature Extraction Strategy
//!
//! Given the limited channels of consumer EEG (5 for Emotiv Insight), we extract
//! features that maximize information while being computationally efficient:
//!
//! - **Band powers**: Power in delta, theta, alpha, beta, gamma frequency bands
//! - **Time-domain statistics**: Mean, variance, skewness, kurtosis per channel
//! - **Hjorth parameters**: Activity, mobility, complexity (signal dynamics)
//! - **Cross-channel**: Coherence and asymmetry between frontal electrodes
//!
//! The choice of features is informed by BCI literature on what's achievable
//! with limited electrodes.

pub mod buffer;
pub mod features;
pub mod filter;
pub mod pipeline;

// ─── Primary API ─────────────────────────────────────────────────────────────

pub use pipeline::{PipelineConfig, PipelineStats, SignalPipeline};

// ─── Component-level API (for testing, advanced use) ─────────────────────────

pub use buffer::{BufferConfig, SampleBuffer, SignalWindow};
pub use features::{FeatureConfig, FeatureExtractor, TemporalState};
pub use filter::{BandpassFilter, FilterChain, FilterConfig, FilterType, NotchFilter};

// ─── Re-exports from neurohid-types ──────────────────────────────────────────

pub use neurohid_types::error::SignalError;
pub use neurohid_types::signal::{ChannelId, FeatureVector, FrequencyBand, Sample, SampleBatch};
