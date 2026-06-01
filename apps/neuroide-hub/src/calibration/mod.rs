//! # Calibration
//!
//! Provides the calibration UI panel, wizard state machine, and calibration
//! games (grid maze, target tracking) as embeddable components within the hub.
//!
//! ## Usage
//!
//! ```no_run
//! use neuroide_hub::calibration::panel::{CalibrationPanel, CalibrationPanelResult};
//! use neuroide_hub::calibration::GameKind;
//! ```

pub mod games;
pub mod panel;
pub mod wizard;

pub use games::GameKind;
