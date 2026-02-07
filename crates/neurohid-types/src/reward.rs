//! # Reward Types
//!
//! Types related to the reward signal, primarily derived from Error-Related
//! Potentials (ErrP). This is the core feedback mechanism that allows the
//! decoder to learn during unconstrained use.
//!
//! ## Key Concepts
//!
//! - **ErrP (Error-Related Potential)**: A brain signal generated when a user
//!   perceives that an action was incorrect. We detect this to provide negative
//!   reward to the decoder.
//!
//! - **Signal Quality**: A measure of how reliable our ErrP detection is at
//!   any given moment. Low quality means we can't trust the reward signal.
//!
//! - **Confidence**: How certain we are that a specific detection is correct.
//!   Distinct from signal quality (system-wide) vs confidence (per-detection).

use serde::{Deserialize, Serialize};
use crate::Timestamp;

/// Overall quality of the biosignal for ErrP detection.
/// This reflects system-wide conditions, not individual detections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalQuality {
    /// Signal is good, ErrP detection should be reliable
    Good,
    /// Signal is acceptable but not ideal
    Acceptable,
    /// Signal is degraded, ErrP detection may be unreliable
    Poor,
    /// Signal is unusable, do not trust ErrP detections
    Unusable,
}

impl SignalQuality {
    /// Whether this quality level is sufficient for ErrP detection
    pub fn is_usable(&self) -> bool {
        matches!(self, SignalQuality::Good | SignalQuality::Acceptable)
    }
    
    /// Convert to a numeric value (for logging, metrics)
    pub fn to_score(&self) -> f32 {
        match self {
            SignalQuality::Good => 1.0,
            SignalQuality::Acceptable => 0.7,
            SignalQuality::Poor => 0.3,
            SignalQuality::Unusable => 0.0,
        }
    }
    
    /// Create from a numeric score (0.0 to 1.0)
    pub fn from_score(score: f32) -> Self {
        if score >= 0.8 {
            SignalQuality::Good
        } else if score >= 0.5 {
            SignalQuality::Acceptable
        } else if score >= 0.2 {
            SignalQuality::Poor
        } else {
            SignalQuality::Unusable
        }
    }
}

/// The result of an ErrP detection attempt for a specific action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrPResult {
    /// Timestamp of the action this ErrP check corresponds to
    pub action_timestamp: Timestamp,
    
    /// Timestamp when the ErrP window was analyzed
    pub detection_timestamp: Timestamp,
    
    /// Probability that an error was perceived (0.0 = no error, 1.0 = definite error)
    pub error_probability: f32,
    
    /// Confidence in this specific detection (0.0 = no confidence, 1.0 = high confidence)
    /// This is distinct from error_probability - we might be 90% confident that
    /// there's a 30% chance of error.
    pub classification_confidence: f32,
    
    /// Current signal quality when detection was made
    pub signal_quality: SignalQuality,
    
    /// Estimated magnitude of the error, if detectable (0.0 = tiny, 1.0 = large)
    /// None if magnitude cannot be determined
    pub estimated_magnitude: Option<f32>,
    
    /// Latency from action to ErrP detection completion (microseconds)
    pub detection_latency_us: i64,
}

impl ErrPResult {
    /// Create a result indicating no error was detected
    pub fn no_error(action_timestamp: Timestamp) -> Self {
        Self {
            action_timestamp,
            detection_timestamp: crate::now_micros(),
            error_probability: 0.0,
            classification_confidence: 0.8,
            signal_quality: SignalQuality::Good,
            estimated_magnitude: None,
            detection_latency_us: 0,
        }
    }
    
    /// Create a result indicating an error was detected
    pub fn error_detected(action_timestamp: Timestamp, probability: f32, confidence: f32) -> Self {
        let detection_timestamp = crate::now_micros();
        Self {
            action_timestamp,
            detection_timestamp,
            error_probability: probability.clamp(0.0, 1.0),
            classification_confidence: confidence.clamp(0.0, 1.0),
            signal_quality: SignalQuality::Good,
            estimated_magnitude: None,
            detection_latency_us: detection_timestamp - action_timestamp,
        }
    }
    
    /// Create a result indicating the signal was unusable
    pub fn unusable(action_timestamp: Timestamp) -> Self {
        Self {
            action_timestamp,
            detection_timestamp: crate::now_micros(),
            error_probability: 0.0,
            classification_confidence: 0.0,
            signal_quality: SignalQuality::Unusable,
            estimated_magnitude: None,
            detection_latency_us: 0,
        }
    }
    
    /// Check if this detection should be trusted
    pub fn is_reliable(&self) -> bool {
        self.signal_quality.is_usable() && self.classification_confidence > 0.5
    }
}

/// The reward signal derived from ErrP detection.
/// This is what gets fed to the RL algorithm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardSignal {
    /// The reward value (typically negative for errors, zero/small positive otherwise)
    pub value: f32,
    
    /// Confidence in this reward signal (RL can weight updates by this)
    pub confidence: f32,
    
    /// Whether explicit user feedback should be requested
    pub request_feedback: bool,
    
    /// The underlying ErrP result this reward was derived from
    pub errp_result: Option<ErrPResult>,
    
    /// Timestamp of the action this reward corresponds to
    pub action_timestamp: Timestamp,
    
    /// Flag indicating special conditions
    pub flag: RewardFlag,
}

impl RewardSignal {
    /// Create a reward signal from an ErrP result using default conversion
    pub fn from_errp(errp: ErrPResult) -> Self {
        let (value, confidence, flag) = match errp.signal_quality {
            SignalQuality::Unusable => (0.0, 0.0, RewardFlag::SignalUnusable),
            SignalQuality::Poor => {
                // Use the signal but with reduced confidence
                let value = -errp.error_probability;
                let confidence = errp.classification_confidence * 0.5;
                (value, confidence, RewardFlag::LowConfidence)
            }
            SignalQuality::Acceptable | SignalQuality::Good => {
                let mut value = -errp.error_probability;
                
                // Scale by magnitude if available
                if let Some(magnitude) = errp.estimated_magnitude {
                    value *= magnitude;
                }
                
                let flag = if errp.classification_confidence < 0.6 {
                    RewardFlag::LowConfidence
                } else {
                    RewardFlag::Normal
                };
                
                (value, errp.classification_confidence, flag)
            }
        };
        
        let request_feedback = matches!(flag, RewardFlag::LowConfidence | RewardFlag::SignalUnusable);
        
        Self {
            value,
            confidence,
            request_feedback,
            errp_result: Some(errp.clone()),
            action_timestamp: errp.action_timestamp,
            flag,
        }
    }
    
    /// Create a neutral reward (no information)
    pub fn neutral(action_timestamp: Timestamp) -> Self {
        Self {
            value: 0.0,
            confidence: 0.0,
            request_feedback: false,
            errp_result: None,
            action_timestamp,
            flag: RewardFlag::NoData,
        }
    }
    
    /// Create a reward from explicit user feedback
    pub fn from_explicit_feedback(action_timestamp: Timestamp, was_correct: bool) -> Self {
        Self {
            value: if was_correct { 0.1 } else { -1.0 },
            confidence: 1.0, // Explicit feedback is always high confidence
            request_feedback: false,
            errp_result: None,
            action_timestamp,
            flag: RewardFlag::ExplicitFeedback,
        }
    }
}

/// Flags indicating special conditions for a reward signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RewardFlag {
    /// Normal ErrP-based reward
    Normal,
    /// Low confidence in the detection
    LowConfidence,
    /// Signal quality was too poor
    SignalUnusable,
    /// No ErrP data available for this action
    NoData,
    /// Reward includes error magnitude weighting
    MagnitudeWeighted,
    /// Reward came from explicit user feedback, not ErrP
    ExplicitFeedback,
}

/// Configuration for ErrP detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrPConfig {
    /// Start of ErrP window relative to action (microseconds, typically positive)
    /// ErrP typically appears 200-300ms after error perception
    pub window_start_us: i64,
    
    /// End of ErrP window relative to action (microseconds)
    /// ErrP window typically extends to 500-600ms
    pub window_end_us: i64,
    
    /// Minimum classification confidence to use the detection
    pub confidence_threshold: f32,
    
    /// Minimum signal quality to attempt detection
    pub min_signal_quality: SignalQuality,
    
    /// Whether to attempt magnitude estimation
    pub estimate_magnitude: bool,
}

impl Default for ErrPConfig {
    fn default() -> Self {
        Self {
            window_start_us: 150_000,  // 150ms after action
            window_end_us: 600_000,    // 600ms after action
            confidence_threshold: 0.5,
            min_signal_quality: SignalQuality::Acceptable,
            estimate_magnitude: true,
        }
    }
}

/// Statistics about ErrP detection performance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrPStats {
    /// Total number of ErrP detection attempts
    pub total_attempts: u64,
    
    /// Number of detections with usable signal quality
    pub usable_detections: u64,
    
    /// Number of errors detected
    pub errors_detected: u64,
    
    /// Average detection latency in microseconds
    pub avg_latency_us: f64,
    
    /// Average classification confidence
    pub avg_confidence: f64,
    
    /// Average signal quality score
    pub avg_quality_score: f64,
}

impl ErrPStats {
    /// Update stats with a new detection result
    pub fn update(&mut self, result: &ErrPResult) {
        self.total_attempts += 1;
        
        if result.signal_quality.is_usable() {
            self.usable_detections += 1;
        }
        
        if result.error_probability > 0.5 {
            self.errors_detected += 1;
        }
        
        // Running averages (simple exponential moving average)
        let alpha = 0.1;
        self.avg_latency_us = self.avg_latency_us * (1.0 - alpha) 
            + result.detection_latency_us as f64 * alpha;
        self.avg_confidence = self.avg_confidence * (1.0 - alpha) 
            + result.classification_confidence as f64 * alpha;
        self.avg_quality_score = self.avg_quality_score * (1.0 - alpha) 
            + result.signal_quality.to_score() as f64 * alpha;
    }
    
    /// Get the error rate (errors / usable detections)
    pub fn error_rate(&self) -> f64 {
        if self.usable_detections == 0 {
            0.0
        } else {
            self.errors_detected as f64 / self.usable_detections as f64
        }
    }
    
    /// Get the usability rate (usable / total)
    pub fn usability_rate(&self) -> f64 {
        if self.total_attempts == 0 {
            0.0
        } else {
            self.usable_detections as f64 / self.total_attempts as f64
        }
    }
}
