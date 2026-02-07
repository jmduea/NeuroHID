//! # Calibration Games
//!
//! This module contains the games used for calibrating the ErrP detector.
//! Each game is designed to elicit clear error-related potentials that we
//! can use to train the classifier.

mod grid_maze;
mod target_tracking;

pub use grid_maze::GridMazeGame;
pub use target_tracking::TargetTrackingGame;
