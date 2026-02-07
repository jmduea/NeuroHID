//! # Signal Task
//!
//! This task sits between the device and the decoder. It receives raw EEG
//! samples, applies digital filters to clean up the signal, and extracts
//! features that the neural network can understand.
//!
//! Think of it like a translator: the device speaks in raw voltage readings,
//! but the decoder needs higher-level summaries like "how much alpha rhythm
//! is present right now?" The signal task does that translation.

use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

use neurohid_types::{
    config::SignalConfig,
    signal::{Sample, FeatureVector},
    reward::ErrPResult,
    error::Result,
};

use crate::service::ServiceState;

/// The signal processing task.
pub struct SignalTask {
    config: SignalConfig,
    sample_rx: mpsc::Receiver<Sample>,
    feature_tx: mpsc::Sender<FeatureVector>,
    errp_rx: mpsc::Receiver<ErrPResult>,
    state: Arc<RwLock<ServiceState>>,
    
    // Internal state for signal processing
    sample_buffer: Vec<Sample>,
    sample_count: u64,
}

impl SignalTask {
    /// Creates a new signal task.
    pub fn new(
        config: SignalConfig,
        sample_rx: mpsc::Receiver<Sample>,
        feature_tx: mpsc::Sender<FeatureVector>,
        errp_rx: mpsc::Receiver<ErrPResult>,
        state: Arc<RwLock<ServiceState>>,
    ) -> Self {
        Self {
            config,
            sample_rx,
            feature_tx,
            errp_rx,
            state,
            sample_buffer: Vec::with_capacity(1024),
            sample_count: 0,
        }
    }
    
    /// Runs the signal task until shutdown is signaled.
    pub async fn run(mut self, mut shutdown: broadcast::Receiver<()>) -> Result<()> {
        tracing::info!("Signal processing task started");
        
        // Calculate how many samples we need for one feature window.
        // For example, with 128 Hz sampling and 500ms window, we need 64 samples.
        let samples_per_window = (self.config.feature_window_ms as f32 / 1000.0 * 128.0) as usize;
        
        // Calculate how many samples between feature extractions.
        // For example, with 50ms step, we extract features every 6.4 samples.
        let samples_per_step = (self.config.feature_step_ms as f32 / 1000.0 * 128.0) as usize;
        let samples_per_step = samples_per_step.max(1); // At least 1
        
        let mut samples_since_extraction = 0usize;
        
        loop {
            tokio::select! {
                // Check for shutdown
                _ = shutdown.recv() => {
                    tracing::info!("Signal task received shutdown signal");
                    break;
                }
                
                // Receive samples from device task
                sample = self.sample_rx.recv() => {
                    match sample {
                        Some(sample) => {
                            self.sample_count += 1;
                            
                            // Add sample to buffer
                            self.sample_buffer.push(sample);
                            samples_since_extraction += 1;
                            
                            // Keep buffer from growing unbounded
                            while self.sample_buffer.len() > self.config.buffer_size_samples {
                                self.sample_buffer.remove(0);
                            }
                            
                            // Check if it's time to extract features
                            if samples_since_extraction >= samples_per_step 
                                && self.sample_buffer.len() >= samples_per_window 
                            {
                                samples_since_extraction = 0;
                                
                                // Extract features from the most recent window
                                let window_start = self.sample_buffer.len() - samples_per_window;
                                let window = &self.sample_buffer[window_start..];
                                
                                let features = self.extract_features(window);
                                
                                // Send features to IPC task
                                if self.feature_tx.send(features).await.is_err() {
                                    tracing::warn!("Feature receiver dropped");
                                    break;
                                }
                            }
                        }
                        None => {
                            // Sample sender dropped
                            tracing::info!("Sample channel closed");
                            break;
                        }
                    }
                }
                
                // Receive ErrP results (for coordinating online learning)
                errp = self.errp_rx.recv() => {
                    if let Some(result) = errp {
                        // In a full implementation, we'd use this to coordinate
                        // online learning and track error rates
                        let mut state = self.state.write().await;
                        if result.error_probability > 0.5 {
                            state.errors_detected += 1;
                        }
                    }
                }
            }
        }
        
        tracing::info!("Signal task processed {} samples", self.sample_count);
        Ok(())
    }
    
    /// Extracts features from a window of samples.
    ///
    /// This is where the signal processing magic happens. We compute various
    /// features that help the decoder understand what's happening in the brain:
    /// - Band power (how much energy in different frequency ranges)
    /// - Statistical measures (mean, variance)
    /// - Temporal features (changes over time)
    fn extract_features(&self, window: &[Sample]) -> FeatureVector {
        // For the MVP, we'll use simple features. A production implementation
        // would use the neurohid-signal crate for proper DSP.
        
        let num_channels = window.first().map(|s| s.channel_count()).unwrap_or(5);
        let mut features = Vec::with_capacity(num_channels * 4);
        
        for ch in 0..num_channels {
            // Get values for this channel
            let values: Vec<f32> = window.iter()
                .filter_map(|s| s.get(ch))
                .collect();
            
            if values.is_empty() {
                // No data for this channel, use zeros
                features.extend_from_slice(&[0.0, 0.0, 0.0, 0.0]);
                continue;
            }
            
            // Compute simple statistics
            let mean = values.iter().sum::<f32>() / values.len() as f32;
            
            let variance = values.iter()
                .map(|v| (v - mean).powi(2))
                .sum::<f32>() / values.len() as f32;
            
            let std_dev = variance.sqrt();
            
            // Compute a simple "power" estimate (sum of squared values)
            let power = values.iter()
                .map(|v| v.powi(2))
                .sum::<f32>() / values.len() as f32;
            
            // Compute the range
            let min = values.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let range = max - min;
            
            features.push(mean);
            features.push(std_dev);
            features.push(power);
            features.push(range);
        }
        
        // Normalize features to reasonable ranges
        for f in &mut features {
            // Clip extreme values
            *f = f.clamp(-500.0, 500.0);
            // Scale to roughly [-1, 1] range
            *f /= 100.0;
        }
        
        FeatureVector::new(features)
    }
}
