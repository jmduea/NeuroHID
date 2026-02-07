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
