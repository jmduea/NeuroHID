//! # Profile Types
//!
//! Types related to user profiles, calibration state, and session management.
//! A profile contains everything needed to use the system for a specific user:
//! their calibration data, trained models, and preferences.

use crate::Timestamp;
use serde::{Deserialize, Serialize};

/// Unique identifier for a user profile.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProfileId(pub String);

impl ProfileId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a new random profile ID
    pub fn generate() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        Self(format!("profile_{}", timestamp))
    }
}

impl Default for ProfileId {
    fn default() -> Self {
        Self::new("default")
    }
}

impl std::fmt::Display for ProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Metadata about a user profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetadata {
    /// The profile identifier
    pub id: ProfileId,

    /// Human-readable name for this profile
    pub name: String,

    /// When the profile was created
    pub created_at: Timestamp,

    /// When the profile was last used
    pub last_used_at: Timestamp,

    /// When calibration was last performed
    pub last_calibrated_at: Option<Timestamp>,

    /// Total time spent using this profile (microseconds)
    pub total_usage_time_us: i64,

    /// Current calibration state
    pub calibration_state: CalibrationState,
}

impl ProfileMetadata {
    /// Create metadata for a new profile
    pub fn new(id: ProfileId, name: String) -> Self {
        let now = crate::now_micros();
        Self {
            id,
            name,
            created_at: now,
            last_used_at: now,
            last_calibrated_at: None,
            total_usage_time_us: 0,
            calibration_state: CalibrationState::NotCalibrated,
        }
    }
}

/// The calibration state of a profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalibrationState {
    /// No calibration has been performed
    NotCalibrated,

    /// Calibration is currently in progress
    InProgress {
        /// Which calibration step we're on
        current_step: CalibrationStep,
        /// Progress within the current step (0.0 to 1.0)
        step_progress: u8, // Stored as percentage 0-100
    },

    /// Calibration completed but quality is poor
    CompletedPoor {
        /// When calibration was completed
        completed_at: Timestamp,
        /// Quality metrics
        quality: CalibrationQuality,
    },

    /// Calibration completed with acceptable quality
    CompletedAcceptable {
        completed_at: Timestamp,
        quality: CalibrationQuality,
    },

    /// Calibration completed with good quality
    CompletedGood {
        completed_at: Timestamp,
        quality: CalibrationQuality,
    },

    /// Calibration needs to be redone (e.g., due to model drift)
    NeedsRecalibration {
        /// Reason recalibration is needed
        reason: String,
        /// Previous calibration quality for reference
        previous_quality: CalibrationQuality,
    },
}

impl CalibrationState {
    /// Check if the profile is ready for use
    pub fn is_ready(&self) -> bool {
        matches!(
            self,
            CalibrationState::CompletedAcceptable { .. } | CalibrationState::CompletedGood { .. }
        )
    }

    /// Check if calibration is in progress
    pub fn is_in_progress(&self) -> bool {
        matches!(self, CalibrationState::InProgress { .. })
    }

    /// Get the calibration quality if completed
    pub fn quality(&self) -> Option<&CalibrationQuality> {
        match self {
            CalibrationState::CompletedPoor { quality, .. } => Some(quality),
            CalibrationState::CompletedAcceptable { quality, .. } => Some(quality),
            CalibrationState::CompletedGood { quality, .. } => Some(quality),
            CalibrationState::NeedsRecalibration {
                previous_quality, ..
            } => Some(previous_quality),
            _ => None,
        }
    }
}

/// Steps in the calibration process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalibrationStep {
    /// Initial signal quality check
    SignalCheck,

    /// ErrP calibration using discrete task (grid maze)
    ErrPDiscrete,

    /// ErrP calibration using continuous task (target tracking)
    ErrPContinuous,

    /// Decoder initial training
    DecoderTraining,

    /// Final validation
    Validation,
}

impl CalibrationStep {
    /// Get the next step in the calibration process
    pub fn next(&self) -> Option<CalibrationStep> {
        match self {
            CalibrationStep::SignalCheck => Some(CalibrationStep::ErrPDiscrete),
            CalibrationStep::ErrPDiscrete => Some(CalibrationStep::ErrPContinuous),
            CalibrationStep::ErrPContinuous => Some(CalibrationStep::DecoderTraining),
            CalibrationStep::DecoderTraining => Some(CalibrationStep::Validation),
            CalibrationStep::Validation => None,
        }
    }

    /// Get the step index (0-based)
    pub fn index(&self) -> usize {
        match self {
            CalibrationStep::SignalCheck => 0,
            CalibrationStep::ErrPDiscrete => 1,
            CalibrationStep::ErrPContinuous => 2,
            CalibrationStep::DecoderTraining => 3,
            CalibrationStep::Validation => 4,
        }
    }

    /// Total number of calibration steps
    pub fn total_steps() -> usize {
        5
    }

    /// Human-readable name for this step
    pub fn display_name(&self) -> &'static str {
        match self {
            CalibrationStep::SignalCheck => "Signal Quality Check",
            CalibrationStep::ErrPDiscrete => "Error Detection (Discrete)",
            CalibrationStep::ErrPContinuous => "Error Detection (Continuous)",
            CalibrationStep::DecoderTraining => "Decoder Training",
            CalibrationStep::Validation => "Validation",
        }
    }
}

/// Quality metrics for a completed calibration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationQuality {
    /// ErrP detection accuracy (cross-validated)
    pub errp_accuracy: f32,

    /// ErrP detection sensitivity (true positive rate)
    pub errp_sensitivity: f32,

    /// ErrP detection specificity (true negative rate)
    pub errp_specificity: f32,

    /// Area under ROC curve for ErrP detection
    pub errp_auc: f32,

    /// Average signal quality during calibration
    pub signal_quality_score: f32,

    /// Number of trials used for calibration
    pub trial_count: u32,

    /// Number of error trials specifically
    pub error_trial_count: u32,
}

// Marker trait implementation
impl Eq for CalibrationQuality {}

impl CalibrationQuality {
    /// Get an overall quality score (0.0 to 1.0)
    pub fn overall_score(&self) -> f32 {
        // Weighted combination of metrics
        let errp_score = self.errp_accuracy * 0.3
            + self.errp_sensitivity * 0.25
            + self.errp_specificity * 0.25
            + self.errp_auc * 0.2;

        // Penalize low trial counts
        let trial_factor = (self.trial_count as f32 / 100.0).min(1.0);

        // Combine with signal quality
        errp_score * 0.7 + self.signal_quality_score * 0.2 + trial_factor * 0.1
    }

    /// Determine the calibration state based on quality metrics
    pub fn to_state(&self, completed_at: Timestamp) -> CalibrationState {
        let score = self.overall_score();

        if score >= 0.75 {
            CalibrationState::CompletedGood {
                completed_at,
                quality: self.clone(),
            }
        } else if score >= 0.55 {
            CalibrationState::CompletedAcceptable {
                completed_at,
                quality: self.clone(),
            }
        } else {
            CalibrationState::CompletedPoor {
                completed_at,
                quality: self.clone(),
            }
        }
    }
}

/// Data collected during a calibration trial.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationTrial {
    /// Trial sequence number
    pub trial_number: u32,

    /// Timestamp when the trial started
    pub timestamp: Timestamp,

    /// Whether this was an error trial (system intentionally made a mistake)
    pub is_error_trial: bool,

    /// What action was intended (by the calibration protocol)
    pub intended_action: String,

    /// What action was executed
    pub executed_action: String,

    /// Duration of the trial in microseconds
    pub duration_us: i64,
}

/// Data from a calibration session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationSessionData {
    /// Session identifier
    pub session_id: String,

    /// Which calibration step this session was for
    pub step: CalibrationStep,

    /// When the session started
    pub started_at: Timestamp,

    /// When the session ended (if completed)
    pub ended_at: Option<Timestamp>,

    /// Trial data collected during the session
    pub trials: Vec<CalibrationTrial>,

    /// Path to the raw EEG data file for this session
    pub eeg_data_path: Option<String>,

    /// Session-level signal quality metrics
    pub signal_quality: Option<f32>,
}

impl CalibrationSessionData {
    /// Create a new calibration session
    pub fn new(step: CalibrationStep) -> Self {
        Self {
            session_id: format!("session_{}_{}", step.index(), crate::now_micros()),
            step,
            started_at: crate::now_micros(),
            ended_at: None,
            trials: Vec::new(),
            eeg_data_path: None,
            signal_quality: None,
        }
    }

    /// Add a trial to the session
    pub fn add_trial(&mut self, trial: CalibrationTrial) {
        self.trials.push(trial);
    }

    /// Get the number of error trials
    pub fn error_trial_count(&self) -> usize {
        self.trials.iter().filter(|t| t.is_error_trial).count()
    }

    /// Get the number of correct trials
    pub fn correct_trial_count(&self) -> usize {
        self.trials.iter().filter(|t| !t.is_error_trial).count()
    }

    /// Check if the session has enough data
    pub fn has_sufficient_data(&self, min_error_trials: usize, min_correct_trials: usize) -> bool {
        self.error_trial_count() >= min_error_trials
            && self.correct_trial_count() >= min_correct_trials
    }
}

/// Session information for usage tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSession {
    /// Session identifier
    pub session_id: String,

    /// When the session started
    pub started_at: Timestamp,

    /// When the session ended (None if still active)
    pub ended_at: Option<Timestamp>,

    /// Number of actions taken during the session
    pub action_count: u64,

    /// Number of ErrP detections during the session
    pub errp_count: u64,

    /// Number of detected errors
    pub error_count: u64,

    /// Average signal quality during the session
    pub avg_signal_quality: f32,
}

impl UsageSession {
    /// Create a new usage session
    pub fn new() -> Self {
        Self {
            session_id: format!("usage_{}", crate::now_micros()),
            started_at: crate::now_micros(),
            ended_at: None,
            action_count: 0,
            errp_count: 0,
            error_count: 0,
            avg_signal_quality: 0.0,
        }
    }

    /// End the session
    pub fn end(&mut self) {
        self.ended_at = Some(crate::now_micros());
    }

    /// Get the session duration in microseconds
    pub fn duration_us(&self) -> i64 {
        let end = self.ended_at.unwrap_or_else(crate::now_micros);
        end - self.started_at
    }

    /// Get the error rate for this session
    pub fn error_rate(&self) -> f64 {
        if self.action_count == 0 {
            0.0
        } else {
            self.error_count as f64 / self.action_count as f64
        }
    }
}

impl Default for UsageSession {
    fn default() -> Self {
        Self::new()
    }
}
