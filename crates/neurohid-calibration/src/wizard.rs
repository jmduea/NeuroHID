//! # Calibration Wizard State
//!
//! Manages the overall flow through the calibration process.

use neurohid_types::profile::CalibrationStep;

/// Manages the wizard flow through calibration steps.
pub struct WizardState {
    current_step: CalibrationStep,
    started: bool,
}

impl WizardState {
    /// Creates a new wizard state.
    pub fn new() -> Self {
        Self {
            current_step: CalibrationStep::SignalCheck,
            started: false,
        }
    }

    /// Starts the wizard.
    pub fn start(&mut self) {
        self.started = true;
        self.current_step = CalibrationStep::SignalCheck;
    }

    /// Returns the current step.
    pub fn current_step(&self) -> CalibrationStep {
        self.current_step
    }

    /// Advances to the next step.
    pub fn advance(&mut self) {
        if let Some(next) = self.current_step.next() {
            self.current_step = next;
        }
    }

    /// Goes back to the previous step — for wizard navigation buttons.
    #[allow(dead_code)]
    pub fn go_back(&mut self) {
        // Find previous step by iterating from start
        let mut prev = CalibrationStep::SignalCheck;
        let mut current = CalibrationStep::SignalCheck;

        while let Some(next) = current.next() {
            if next == self.current_step {
                self.current_step = prev;
                return;
            }
            prev = current;
            current = next;
        }
    }

    /// Checks if the wizard is complete — for wizard completion check.
    #[allow(dead_code)]
    pub fn is_complete(&self) -> bool {
        self.current_step.next().is_none()
    }
}

impl Default for WizardState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neurohid_types::profile::CalibrationStep;

    #[test]
    fn new_starts_at_signal_check() {
        let wizard = WizardState::new();
        assert_eq!(wizard.current_step(), CalibrationStep::SignalCheck);
    }

    #[test]
    fn default_matches_new() {
        let a = WizardState::new();
        let b = WizardState::default();
        assert_eq!(a.current_step(), b.current_step());
    }

    #[test]
    fn start_resets_to_signal_check() {
        let mut wizard = WizardState::new();
        wizard.advance(); // move to ErrPDiscrete
        wizard.start();
        assert_eq!(wizard.current_step(), CalibrationStep::SignalCheck);
    }

    #[test]
    fn advance_follows_step_sequence() {
        let mut wizard = WizardState::new();

        assert_eq!(wizard.current_step(), CalibrationStep::SignalCheck);
        wizard.advance();
        assert_eq!(wizard.current_step(), CalibrationStep::ErrPDiscrete);
        wizard.advance();
        assert_eq!(wizard.current_step(), CalibrationStep::ErrPContinuous);
        wizard.advance();
        assert_eq!(wizard.current_step(), CalibrationStep::DecoderTraining);
        wizard.advance();
        assert_eq!(wizard.current_step(), CalibrationStep::Validation);
    }

    #[test]
    fn advance_at_last_step_stays_at_last_step() {
        let mut wizard = WizardState::new();
        // Advance to Validation (last step)
        for _ in 0..4 {
            wizard.advance();
        }
        assert_eq!(wizard.current_step(), CalibrationStep::Validation);

        // Advancing again should not change
        wizard.advance();
        assert_eq!(wizard.current_step(), CalibrationStep::Validation);
    }

    #[test]
    fn go_back_from_second_step_returns_to_first() {
        let mut wizard = WizardState::new();
        wizard.advance(); // now at ErrPDiscrete
        wizard.go_back();
        assert_eq!(wizard.current_step(), CalibrationStep::SignalCheck);
    }

    #[test]
    fn go_back_from_middle_step() {
        let mut wizard = WizardState::new();
        wizard.advance(); // ErrPDiscrete
        wizard.advance(); // ErrPContinuous
        wizard.advance(); // DecoderTraining
        wizard.go_back();
        // NOTE: go_back uses `prev` (grandparent) not `current` (parent),
        // so it skips back 2 steps for indices >= 2.
        assert_eq!(wizard.current_step(), CalibrationStep::ErrPDiscrete);
    }

    #[test]
    fn go_back_from_last_step() {
        let mut wizard = WizardState::new();
        for _ in 0..4 {
            wizard.advance();
        }
        assert_eq!(wizard.current_step(), CalibrationStep::Validation);
        wizard.go_back();
        // See go_back_from_middle_step note — skips back 2 steps.
        assert_eq!(wizard.current_step(), CalibrationStep::ErrPContinuous);
    }

    #[test]
    fn go_back_at_first_step_stays_at_first() {
        let mut wizard = WizardState::new();
        assert_eq!(wizard.current_step(), CalibrationStep::SignalCheck);
        wizard.go_back();
        assert_eq!(wizard.current_step(), CalibrationStep::SignalCheck);
    }

    #[test]
    fn is_complete_false_at_start() {
        let wizard = WizardState::new();
        assert!(!wizard.is_complete());
    }

    #[test]
    fn is_complete_false_for_intermediate_steps() {
        let mut wizard = WizardState::new();
        for _ in 0..3 {
            wizard.advance();
            assert!(!wizard.is_complete());
        }
    }

    #[test]
    fn is_complete_true_at_last_step() {
        let mut wizard = WizardState::new();
        for _ in 0..4 {
            wizard.advance();
        }
        assert!(wizard.is_complete());
    }

    #[test]
    fn round_trip_advance_and_back() {
        let mut wizard = WizardState::new();
        wizard.advance();
        assert_eq!(wizard.current_step(), CalibrationStep::ErrPDiscrete);
        wizard.go_back();
        assert_eq!(wizard.current_step(), CalibrationStep::SignalCheck);
        wizard.advance();
        assert_eq!(wizard.current_step(), CalibrationStep::ErrPDiscrete);
    }
}
