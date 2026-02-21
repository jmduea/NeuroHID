//! # NeuroHID Calibration Library
//!
//! Provides the calibration UI panel, wizard state machine, and calibration
//! games (grid maze, target tracking) as embeddable components. The hub GUI
//! imports this library to render calibration within its central panel.
//!
//! ## Usage
//!
//! ```no_run
//! use neurohid_calibration::panel::{CalibrationPanel, CalibrationPanelResult};
//! use neurohid_calibration::GameKind;
//! ```

pub mod games;
pub mod panel;
pub mod wizard;

pub use games::GameKind;
