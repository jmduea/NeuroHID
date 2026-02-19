//! # Grid Maze Game
//!
//! A simple maze navigation game for calibrating ErrP detection with discrete
//! actions. The player navigates a grid-based maze, and the system occasionally
//! executes the wrong action to elicit error-related potentials.
//!
//! ## How It Works
//!
//! The player sees a grid with their position (blue) and a goal (green). They're
//! instructed to think about moving in a direction. The system then either:
//! - Executes the correct move (70% of the time) - no ErrP expected
//! - Executes the wrong move (30% of the time) - ErrP expected
//!
//! By comparing brain signals after correct vs. incorrect actions, we can train
//! the ErrP classifier.

use eframe::egui;

/// Grid maze game state.
pub struct GridMazeGame {
    /// Grid size (width and height)
    grid_size: i32,

    /// Current player position
    player_pos: (i32, i32),

    /// Goal position
    goal_pos: (i32, i32),

    /// Current trial number
    trial_number: u32,

    /// Total trials to complete
    total_trials: u32,

    /// Number of correct trials
    correct_trials: u32,

    /// Number of error trials (deliberate mistakes)
    error_trials: u32,

    /// Current game phase
    phase: GamePhase,

    /// Direction player should think about
    intended_direction: Option<Direction>,

    /// Direction that will actually be executed
    actual_direction: Option<Direction>,

    /// Time remaining in current phase (seconds)
    phase_timer: f32,

    /// Whether the current trial is an error trial
    is_error_trial: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum GamePhase {
    /// Show instruction for which direction to think
    ShowInstruction,
    /// Player is thinking about the direction
    Thinking,
    /// Execute the action (correct or error)
    Executing,
    /// Show feedback about what happened
    Feedback,
    /// Brief pause between trials
    InterTrial,
    /// Game complete
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn as_delta(&self) -> (i32, i32) {
        match self {
            Direction::Up => (0, -1),
            Direction::Down => (0, 1),
            Direction::Left => (-1, 0),
            Direction::Right => (1, 0),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Direction::Up => "UP",
            Direction::Down => "DOWN",
            Direction::Left => "LEFT",
            Direction::Right => "RIGHT",
        }
    }

    fn opposite(&self) -> Direction {
        match self {
            Direction::Up => Direction::Down,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }
}

impl GridMazeGame {
    /// Creates a new grid maze game.
    pub fn new() -> Self {
        let grid_size = 5;
        let player_pos = (grid_size / 2, grid_size / 2);
        let goal_pos = Self::random_goal(grid_size, player_pos);

        Self {
            grid_size,
            player_pos,
            goal_pos,
            trial_number: 0,
            total_trials: 50, // Minimum for decent calibration
            correct_trials: 0,
            error_trials: 0,
            phase: GamePhase::ShowInstruction,
            intended_direction: None,
            actual_direction: None,
            phase_timer: 2.0,
            is_error_trial: false,
        }
    }

    fn random_goal(grid_size: i32, player_pos: (i32, i32)) -> (i32, i32) {
        loop {
            let pos = (
                rand::random_range(0..grid_size),
                rand::random_range(0..grid_size),
            );
            // Goal should be at least 2 cells away from player
            let dist = (pos.0 - player_pos.0).abs() + (pos.1 - player_pos.1).abs();
            if dist >= 2 {
                return pos;
            }
        }
    }

    fn choose_direction_toward_goal(&self) -> Direction {
        let dx = self.goal_pos.0 - self.player_pos.0;
        let dy = self.goal_pos.1 - self.player_pos.1;

        // Pick the direction that moves us closer to the goal
        if dx.abs() > dy.abs() {
            if dx > 0 {
                Direction::Right
            } else {
                Direction::Left
            }
        } else if dy > 0 {
            Direction::Down
        } else {
            Direction::Up
        }
    }

    fn start_new_trial(&mut self) {
        self.trial_number += 1;

        // Decide the intended direction (toward goal)
        self.intended_direction = Some(self.choose_direction_toward_goal());

        // Decide if this is an error trial (30% error rate)
        self.is_error_trial = rand::random_bool(0.3);

        // Determine actual direction
        self.actual_direction = if self.is_error_trial {
            // Pick a different direction (usually opposite)
            Some(self.intended_direction.unwrap().opposite())
        } else {
            self.intended_direction
        };

        self.phase = GamePhase::ShowInstruction;
        self.phase_timer = 1.5;
    }

    fn apply_move(&mut self) {
        if let Some(dir) = self.actual_direction {
            let (dx, dy) = dir.as_delta();
            let new_x = (self.player_pos.0 + dx).clamp(0, self.grid_size - 1);
            let new_y = (self.player_pos.1 + dy).clamp(0, self.grid_size - 1);
            self.player_pos = (new_x, new_y);

            // Track trial types
            if self.is_error_trial {
                self.error_trials += 1;
            } else {
                self.correct_trials += 1;
            }
        }
    }

    fn check_goal_reached(&mut self) {
        if self.player_pos == self.goal_pos {
            // Reset positions for next series
            self.player_pos = (self.grid_size / 2, self.grid_size / 2);
            self.goal_pos = Self::random_goal(self.grid_size, self.player_pos);
        }
    }

    /// Renders the game and returns true when complete.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        // Update phase timer
        let dt = ctx.input(|i| i.stable_dt);
        self.phase_timer -= dt;

        // Phase transitions
        if self.phase_timer <= 0.0 {
            match self.phase {
                GamePhase::ShowInstruction => {
                    self.phase = GamePhase::Thinking;
                    self.phase_timer = 2.0;
                }
                GamePhase::Thinking => {
                    self.phase = GamePhase::Executing;
                    self.phase_timer = 0.5;
                    self.apply_move();
                }
                GamePhase::Executing => {
                    self.phase = GamePhase::Feedback;
                    self.phase_timer = 1.5;
                }
                GamePhase::Feedback => {
                    self.check_goal_reached();

                    if self.trial_number >= self.total_trials {
                        self.phase = GamePhase::Complete;
                    } else {
                        self.phase = GamePhase::InterTrial;
                        self.phase_timer = 1.0;
                    }
                }
                GamePhase::InterTrial => {
                    self.start_new_trial();
                }
                GamePhase::Complete => {
                    // Stay in complete state
                }
            }
        }

        // Start first trial if needed
        if self.trial_number == 0 && self.phase == GamePhase::ShowInstruction {
            self.start_new_trial();
        }

        // Render UI
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                // Progress
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Trial {}/{} | Correct: {} | Error: {}",
                        self.trial_number,
                        self.total_trials,
                        self.correct_trials,
                        self.error_trials
                    ));
                });

                ui.add_space(20.0);

                // Instruction based on phase
                match self.phase {
                    GamePhase::ShowInstruction => {
                        if let Some(dir) = self.intended_direction {
                            ui.heading(format!("Think: {}", dir.as_str()));
                        }
                    }
                    GamePhase::Thinking => {
                        if let Some(dir) = self.intended_direction {
                            ui.heading(format!("Keep thinking: {}", dir.as_str()));
                        }
                        ui.label("Focus on the movement...");
                    }
                    GamePhase::Executing => {
                        ui.heading("Moving...");
                    }
                    GamePhase::Feedback => {
                        if self.is_error_trial {
                            ui.colored_label(egui::Color32::RED, "Wrong direction!");
                        } else {
                            ui.colored_label(egui::Color32::GREEN, "Correct!");
                        }
                    }
                    GamePhase::InterTrial => {
                        ui.heading("Get ready...");
                    }
                    GamePhase::Complete => {
                        ui.heading("Game Complete!");
                        ui.label(format!(
                            "Completed {} trials ({} correct, {} error)",
                            self.total_trials, self.correct_trials, self.error_trials
                        ));
                    }
                }

                ui.add_space(30.0);

                // Draw the grid
                self.draw_grid(ui);
            });
        });

        self.phase == GamePhase::Complete
    }

    /// Returns (correct_trials, error_trials) for quality assessment.
    pub fn quality_metrics(&self) -> (u32, u32) {
        (self.correct_trials, self.error_trials)
    }

    fn draw_grid(&self, ui: &mut egui::Ui) {
        let available_size = ui.available_size();
        let grid_pixels = available_size.x.min(available_size.y).min(400.0);
        let cell_size = grid_pixels / self.grid_size as f32;

        let (response, painter) =
            ui.allocate_painter(egui::vec2(grid_pixels, grid_pixels), egui::Sense::hover());

        let rect = response.rect;

        // Draw grid cells
        for x in 0..self.grid_size {
            for y in 0..self.grid_size {
                let cell_rect = egui::Rect::from_min_size(
                    egui::pos2(
                        rect.min.x + x as f32 * cell_size,
                        rect.min.y + y as f32 * cell_size,
                    ),
                    egui::vec2(cell_size, cell_size),
                );

                // Determine cell color
                let color = if (x, y) == self.player_pos {
                    egui::Color32::from_rgb(50, 100, 200) // Player: blue
                } else if (x, y) == self.goal_pos {
                    egui::Color32::from_rgb(50, 200, 100) // Goal: green
                } else {
                    egui::Color32::from_rgb(60, 60, 70) // Empty: dark gray
                };

                painter.rect_filled(cell_rect.shrink(2.0), 4.0, color);
                painter.rect_stroke(
                    cell_rect.shrink(2.0),
                    4.0,
                    egui::Stroke::new(1.0, egui::Color32::GRAY),
                    egui::StrokeKind::Outside,
                );
            }
        }
    }
}

impl Default for GridMazeGame {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Direction tests ---

    #[test]
    fn direction_as_delta_up() {
        assert_eq!(Direction::Up.as_delta(), (0, -1));
    }

    #[test]
    fn direction_as_delta_down() {
        assert_eq!(Direction::Down.as_delta(), (0, 1));
    }

    #[test]
    fn direction_as_delta_left() {
        assert_eq!(Direction::Left.as_delta(), (-1, 0));
    }

    #[test]
    fn direction_as_delta_right() {
        assert_eq!(Direction::Right.as_delta(), (1, 0));
    }

    #[test]
    fn direction_as_str_values() {
        assert_eq!(Direction::Up.as_str(), "UP");
        assert_eq!(Direction::Down.as_str(), "DOWN");
        assert_eq!(Direction::Left.as_str(), "LEFT");
        assert_eq!(Direction::Right.as_str(), "RIGHT");
    }

    #[test]
    fn direction_opposite_is_symmetric() {
        assert_eq!(Direction::Up.opposite(), Direction::Down);
        assert_eq!(Direction::Down.opposite(), Direction::Up);
        assert_eq!(Direction::Left.opposite(), Direction::Right);
        assert_eq!(Direction::Right.opposite(), Direction::Left);
    }

    #[test]
    fn direction_double_opposite_is_identity() {
        for dir in [
            Direction::Up,
            Direction::Down,
            Direction::Left,
            Direction::Right,
        ] {
            assert_eq!(dir.opposite().opposite(), dir);
        }
    }

    // --- GridMazeGame construction tests ---

    #[test]
    fn new_game_has_correct_defaults() {
        let game = GridMazeGame::new();
        assert_eq!(game.grid_size, 5);
        assert_eq!(game.player_pos, (2, 2)); // grid_size / 2
        assert_eq!(game.trial_number, 0);
        assert_eq!(game.total_trials, 50);
        assert_eq!(game.correct_trials, 0);
        assert_eq!(game.error_trials, 0);
        assert_eq!(game.phase, GamePhase::ShowInstruction);
    }

    #[test]
    fn default_matches_new() {
        let a = GridMazeGame::new();
        let b = GridMazeGame::default();
        assert_eq!(a.grid_size, b.grid_size);
        assert_eq!(a.player_pos, b.player_pos);
        assert_eq!(a.trial_number, b.trial_number);
        assert_eq!(a.total_trials, b.total_trials);
    }

    // --- Goal placement tests ---

    #[test]
    fn random_goal_is_at_least_two_away() {
        let player = (2, 2);
        for _ in 0..100 {
            let goal = GridMazeGame::random_goal(5, player);
            let dist = (goal.0 - player.0).abs() + (goal.1 - player.1).abs();
            assert!(
                dist >= 2,
                "goal {goal:?} too close to player {player:?} (dist={dist})"
            );
        }
    }

    #[test]
    fn random_goal_stays_within_grid() {
        for _ in 0..100 {
            let goal = GridMazeGame::random_goal(5, (2, 2));
            assert!(goal.0 >= 0 && goal.0 < 5);
            assert!(goal.1 >= 0 && goal.1 < 5);
        }
    }

    // --- Direction toward goal tests ---

    #[test]
    fn choose_direction_toward_goal_right() {
        let mut game = GridMazeGame::new();
        game.player_pos = (1, 2);
        game.goal_pos = (4, 2);
        assert_eq!(game.choose_direction_toward_goal(), Direction::Right);
    }

    #[test]
    fn choose_direction_toward_goal_left() {
        let mut game = GridMazeGame::new();
        game.player_pos = (3, 2);
        game.goal_pos = (0, 2);
        assert_eq!(game.choose_direction_toward_goal(), Direction::Left);
    }

    #[test]
    fn choose_direction_toward_goal_down() {
        let mut game = GridMazeGame::new();
        game.player_pos = (2, 1);
        game.goal_pos = (2, 4);
        assert_eq!(game.choose_direction_toward_goal(), Direction::Down);
    }

    #[test]
    fn choose_direction_toward_goal_up() {
        let mut game = GridMazeGame::new();
        game.player_pos = (2, 3);
        game.goal_pos = (2, 0);
        assert_eq!(game.choose_direction_toward_goal(), Direction::Up);
    }

    #[test]
    fn choose_direction_prefers_horizontal_when_equal() {
        let mut game = GridMazeGame::new();
        game.player_pos = (0, 0);
        game.goal_pos = (2, 2);
        // dx=2, dy=2 — abs equal, so horizontal wins (dx.abs() > dy.abs() is false,
        // falls to dy > 0 ⇒ Down)
        assert_eq!(game.choose_direction_toward_goal(), Direction::Down);
    }

    // --- Movement and boundary tests ---

    #[test]
    fn apply_move_correct_trial_increments_correct_count() {
        let mut game = GridMazeGame::new();
        game.actual_direction = Some(Direction::Right);
        game.is_error_trial = false;
        game.apply_move();
        assert_eq!(game.correct_trials, 1);
        assert_eq!(game.error_trials, 0);
    }

    #[test]
    fn apply_move_error_trial_increments_error_count() {
        let mut game = GridMazeGame::new();
        game.actual_direction = Some(Direction::Left);
        game.is_error_trial = true;
        game.apply_move();
        assert_eq!(game.correct_trials, 0);
        assert_eq!(game.error_trials, 1);
    }

    #[test]
    fn apply_move_updates_position() {
        let mut game = GridMazeGame::new();
        game.player_pos = (2, 2);
        game.actual_direction = Some(Direction::Right);
        game.is_error_trial = false;
        game.apply_move();
        assert_eq!(game.player_pos, (3, 2));
    }

    #[test]
    fn apply_move_clamps_to_grid_upper_boundary() {
        let mut game = GridMazeGame::new();
        game.player_pos = (0, 0);
        game.actual_direction = Some(Direction::Up);
        game.is_error_trial = false;
        game.apply_move();
        assert_eq!(game.player_pos, (0, 0)); // clamped at 0
    }

    #[test]
    fn apply_move_clamps_to_grid_lower_boundary() {
        let mut game = GridMazeGame::new();
        game.player_pos = (4, 4);
        game.actual_direction = Some(Direction::Down);
        game.is_error_trial = false;
        game.apply_move();
        assert_eq!(game.player_pos, (4, 4)); // clamped at grid_size-1
    }

    #[test]
    fn apply_move_clamps_to_grid_left_boundary() {
        let mut game = GridMazeGame::new();
        game.player_pos = (0, 2);
        game.actual_direction = Some(Direction::Left);
        game.is_error_trial = false;
        game.apply_move();
        assert_eq!(game.player_pos, (0, 2));
    }

    #[test]
    fn apply_move_clamps_to_grid_right_boundary() {
        let mut game = GridMazeGame::new();
        game.player_pos = (4, 2);
        game.actual_direction = Some(Direction::Right);
        game.is_error_trial = false;
        game.apply_move();
        assert_eq!(game.player_pos, (4, 2));
    }

    #[test]
    fn apply_move_with_no_direction_does_nothing() {
        let mut game = GridMazeGame::new();
        let pos_before = game.player_pos;
        game.actual_direction = None;
        game.apply_move();
        assert_eq!(game.player_pos, pos_before);
        assert_eq!(game.correct_trials, 0);
        assert_eq!(game.error_trials, 0);
    }

    // --- Goal reached tests ---

    #[test]
    fn check_goal_reached_resets_when_at_goal() {
        let mut game = GridMazeGame::new();
        game.goal_pos = (3, 3);
        game.player_pos = (3, 3);
        game.check_goal_reached();
        // Player should be reset to center
        assert_eq!(game.player_pos, (2, 2));
        // Goal should differ from player
        assert_ne!(game.player_pos, game.goal_pos);
    }

    #[test]
    fn check_goal_reached_no_op_when_not_at_goal() {
        let mut game = GridMazeGame::new();
        game.player_pos = (1, 1);
        game.goal_pos = (4, 4);
        let goal_before = game.goal_pos;
        game.check_goal_reached();
        assert_eq!(game.player_pos, (1, 1));
        assert_eq!(game.goal_pos, goal_before);
    }

    // --- Quality metrics tests ---

    #[test]
    fn quality_metrics_reflects_trial_counts() {
        let mut game = GridMazeGame::new();
        game.correct_trials = 35;
        game.error_trials = 15;
        assert_eq!(game.quality_metrics(), (35, 15));
    }

    #[test]
    fn quality_metrics_zero_initially() {
        let game = GridMazeGame::new();
        assert_eq!(game.quality_metrics(), (0, 0));
    }

    // --- Trial lifecycle tests ---

    #[test]
    fn start_new_trial_increments_trial_number() {
        let mut game = GridMazeGame::new();
        game.start_new_trial();
        assert_eq!(game.trial_number, 1);
        game.start_new_trial();
        assert_eq!(game.trial_number, 2);
    }

    #[test]
    fn start_new_trial_sets_intended_direction() {
        let mut game = GridMazeGame::new();
        game.start_new_trial();
        assert!(game.intended_direction.is_some());
    }

    #[test]
    fn start_new_trial_sets_actual_direction() {
        let mut game = GridMazeGame::new();
        game.start_new_trial();
        assert!(game.actual_direction.is_some());
    }

    #[test]
    fn start_new_trial_resets_to_show_instruction_phase() {
        let mut game = GridMazeGame::new();
        game.phase = GamePhase::InterTrial;
        game.start_new_trial();
        assert_eq!(game.phase, GamePhase::ShowInstruction);
    }

    #[test]
    fn error_trial_uses_opposite_direction() {
        let mut game = GridMazeGame::new();
        // Run many trials to get at least one error trial
        for _ in 0..200 {
            game.trial_number = 0; // reset so start_new_trial works
            game.start_new_trial();
            if game.is_error_trial {
                let intended = game.intended_direction.unwrap();
                let actual = game.actual_direction.unwrap();
                assert_eq!(actual, intended.opposite());
                return;
            }
        }
        panic!("No error trial produced in 200 iterations (statistically near-impossible)");
    }

    #[test]
    fn correct_trial_uses_intended_direction() {
        let mut game = GridMazeGame::new();
        for _ in 0..200 {
            game.trial_number = 0;
            game.start_new_trial();
            if !game.is_error_trial {
                assert_eq!(game.actual_direction, game.intended_direction);
                return;
            }
        }
        panic!("No correct trial produced in 200 iterations");
    }
}
