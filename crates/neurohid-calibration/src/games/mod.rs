//! # Calibration Games
//!
//! This module contains the games used for calibrating the ErrP detector.
//! Each game is designed to elicit clear error-related potentials that we
//! can use to train the classifier.

mod grid_maze;
mod target_tracking;

/// Identifies which calibration game to run (single-game flow from hub).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameKind {
    GridMaze,
    TargetTracking,
}

impl GameKind {
    /// All calibration games (for hub game list).
    pub fn all() -> impl Iterator<Item = GameKind> {
        [GameKind::GridMaze, GameKind::TargetTracking].into_iter()
    }

    pub fn display_name(self) -> &'static str {
        match self {
            GameKind::GridMaze => "Grid Maze",
            GameKind::TargetTracking => "Target Tracking",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            GameKind::GridMaze => "ErrP calibration via grid navigation task.",
            GameKind::TargetTracking => "ErrP calibration via target tracking with perturbations.",
        }
    }
}

pub use grid_maze::GridMazeGame;
pub use target_tracking::TargetTrackingGame;
