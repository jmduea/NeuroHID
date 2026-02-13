//! # Continuous Learning Types
//!
//! Types used by the runtime session logger and candidate model guardrails.

use serde::{Deserialize, Serialize};

use crate::{action::Action, now_micros, Timestamp};

/// One training episode captured from the live runtime loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingEpisode {
    /// Source timestamp of the originating feature/action decision.
    pub timestamp: Timestamp,
    /// Flattened feature vector consumed by the decoder.
    pub feature_values: Vec<f32>,
    /// Action emitted by the runtime decoder.
    pub action: Action,
    /// Decoder confidence attached to the action.
    pub decoder_confidence: f32,
    /// Signal quality estimate at decision time.
    pub signal_quality: f32,
    /// Decoder model version used for this decision.
    pub decoder_model_version: Option<String>,
    /// Optional ErrP probability aligned to the episode.
    pub errp_error_probability: Option<f32>,
    /// Optional ErrP detector confidence aligned to the episode.
    pub errp_confidence: Option<f32>,
}

/// Session-level collection of recorded training episodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingSessionLog {
    /// Stable session identifier (typically startup timestamp).
    pub session_id: String,
    /// Profile associated with this session.
    pub profile_id: String,
    /// Session start timestamp.
    pub started_at: Timestamp,
    /// Last update timestamp.
    pub last_updated_at: Timestamp,
    /// Recorded episodes.
    pub episodes: Vec<TrainingEpisode>,
}

impl TrainingSessionLog {
    /// Create an empty session log.
    pub fn new(session_id: String, profile_id: String, started_at: Timestamp) -> Self {
        Self {
            session_id,
            profile_id,
            started_at,
            last_updated_at: started_at,
            episodes: Vec::new(),
        }
    }

    /// Append one episode and advance `last_updated_at`.
    pub fn append_episode(&mut self, episode: TrainingEpisode) {
        self.last_updated_at = now_micros();
        self.episodes.push(episode);
    }
}

/// Candidate model quality metrics produced by training workers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateModelMetrics {
    /// Number of holdout samples used for evaluation.
    pub holdout_sample_count: usize,
    /// Holdout set accuracy in `[0.0, 1.0]`.
    pub holdout_accuracy: f32,
    /// Holdout set loss (lower is better).
    pub holdout_loss: f32,
    /// Candidate decode latency p95 measured in microseconds.
    pub decode_latency_p95_us: u64,
    /// Candidate generation timestamp.
    pub generated_at: Timestamp,
}

impl CandidateModelMetrics {
    /// Structural validation before applying guardrails.
    pub fn validate(&self) -> Result<(), String> {
        if self.holdout_sample_count == 0 {
            return Err("holdout_sample_count must be greater than zero".to_string());
        }
        if !(0.0..=1.0).contains(&self.holdout_accuracy) {
            return Err("holdout_accuracy must be in [0.0, 1.0]".to_string());
        }
        if !self.holdout_loss.is_finite() || self.holdout_loss < 0.0 {
            return Err("holdout_loss must be a finite non-negative value".to_string());
        }
        if self.decode_latency_p95_us == 0 {
            return Err("decode_latency_p95_us must be greater than zero".to_string());
        }
        if self.generated_at <= 0 {
            return Err("generated_at must be a positive timestamp".to_string());
        }
        Ok(())
    }

    /// Evaluate candidate quality against safety guardrails.
    pub fn evaluate(&self, guardrails: &CandidateGuardrails) -> Result<(), String> {
        self.validate()?;
        if self.holdout_accuracy < guardrails.min_holdout_accuracy {
            return Err(format!(
                "holdout_accuracy {} below minimum {}",
                self.holdout_accuracy, guardrails.min_holdout_accuracy
            ));
        }
        if self.holdout_loss > guardrails.max_holdout_loss {
            return Err(format!(
                "holdout_loss {} above maximum {}",
                self.holdout_loss, guardrails.max_holdout_loss
            ));
        }
        if self.decode_latency_p95_us > guardrails.max_decode_latency_p95_us {
            return Err(format!(
                "decode_latency_p95_us {} above maximum {}",
                self.decode_latency_p95_us, guardrails.max_decode_latency_p95_us
            ));
        }
        Ok(())
    }
}

/// Runtime safety gates for candidate model activation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidateGuardrails {
    /// Minimum holdout accuracy required before activation.
    pub min_holdout_accuracy: f32,
    /// Maximum holdout loss accepted before activation.
    pub max_holdout_loss: f32,
    /// Maximum decode latency p95 accepted before activation.
    pub max_decode_latency_p95_us: u64,
}

impl Default for CandidateGuardrails {
    fn default() -> Self {
        Self {
            min_holdout_accuracy: 0.55,
            max_holdout_loss: 1.5,
            max_decode_latency_p95_us: 120_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CandidateGuardrails, CandidateModelMetrics, TrainingSessionLog};

    #[test]
    fn candidate_metrics_validate_and_pass_guardrails() {
        let metrics = CandidateModelMetrics {
            holdout_sample_count: 200,
            holdout_accuracy: 0.72,
            holdout_loss: 0.85,
            decode_latency_p95_us: 40_000,
            generated_at: 1,
        };
        let guardrails = CandidateGuardrails::default();
        assert!(metrics.validate().is_ok());
        assert!(metrics.evaluate(&guardrails).is_ok());
    }

    #[test]
    fn candidate_metrics_reject_poor_accuracy() {
        let metrics = CandidateModelMetrics {
            holdout_sample_count: 50,
            holdout_accuracy: 0.2,
            holdout_loss: 0.9,
            decode_latency_p95_us: 20_000,
            generated_at: 1,
        };
        let guardrails = CandidateGuardrails::default();
        assert!(metrics.evaluate(&guardrails).is_err());
    }

    #[test]
    fn session_log_append_updates_timestamp() {
        let mut log = TrainingSessionLog::new("s1".to_string(), "p1".to_string(), 1);
        let before = log.last_updated_at;
        log.append_episode(super::TrainingEpisode {
            timestamp: 2,
            feature_values: vec![0.1, 0.2],
            action: crate::Action::none(),
            decoder_confidence: 0.9,
            signal_quality: 0.8,
            decoder_model_version: Some("1.0.0".to_string()),
            errp_error_probability: None,
            errp_confidence: None,
        });
        assert_eq!(log.episodes.len(), 1);
        assert!(log.last_updated_at >= before);
    }
}
