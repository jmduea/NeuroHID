//! # Target Tracking Game
//!
//! A continuous control game for calibrating ErrP detection during smooth
//! cursor movements. The player tracks a moving target, and the system
//! occasionally injects perturbations to elicit error-related potentials.
//!
//! This complements the Grid Maze game by capturing ErrPs during continuous
//! control rather than discrete actions, which more closely resembles actual
//! mouse control use cases.

use armas::components::Progress;
use armas::prelude::{ArmasContextExt, Button, ButtonSize, ButtonVariant};
use eframe::egui;

/// Target tracking game state.
pub struct TargetTrackingGame {
    /// Current cursor position (normalized 0-1)
    cursor_pos: (f32, f32),

    /// Target position (normalized 0-1)
    target_pos: (f32, f32),

    /// Target velocity
    target_velocity: (f32, f32),

    /// Game duration in seconds
    game_duration: f32,

    /// Time elapsed
    elapsed_time: f32,

    /// Time until next perturbation
    next_perturbation_time: f32,

    /// Whether a perturbation is currently active
    perturbation_active: bool,

    /// Perturbation end time
    perturbation_end_time: f32,

    /// Number of perturbations applied
    perturbation_count: u32,

    /// Accumulated tracking error
    total_error: f32,

    /// Number of samples for error calculation
    error_samples: u32,

    /// Game phase
    phase: GamePhase,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum GamePhase {
    /// Instructions before starting
    Instructions,
    /// Active tracking
    Tracking,
    /// Game complete
    Complete,
}

impl TargetTrackingGame {
    /// Creates a new target tracking game.
    pub fn new() -> Self {
        Self {
            cursor_pos: (0.5, 0.5),
            target_pos: (0.5, 0.5),
            target_velocity: (0.1, 0.05),
            game_duration: 120.0, // 2 minutes
            elapsed_time: 0.0,
            next_perturbation_time: 3.0,
            perturbation_active: false,
            perturbation_end_time: 0.0,
            perturbation_count: 0,
            total_error: 0.0,
            error_samples: 0,
            phase: GamePhase::Instructions,
        }
    }

    fn update_target(&mut self, dt: f32) {
        // Move target smoothly
        self.target_pos.0 += self.target_velocity.0 * dt;
        self.target_pos.1 += self.target_velocity.1 * dt;

        // Bounce off edges
        if self.target_pos.0 <= 0.1 || self.target_pos.0 >= 0.9 {
            self.target_velocity.0 = -self.target_velocity.0;
            // Add some randomness to velocity
            self.target_velocity.1 += (rand::random::<f32>() - 0.5) * 0.05;
        }
        if self.target_pos.1 <= 0.1 || self.target_pos.1 >= 0.9 {
            self.target_velocity.1 = -self.target_velocity.1;
            self.target_velocity.0 += (rand::random::<f32>() - 0.5) * 0.05;
        }

        // Clamp velocity to reasonable range
        let speed = (self.target_velocity.0.powi(2) + self.target_velocity.1.powi(2)).sqrt();
        if speed > 0.2 {
            self.target_velocity.0 *= 0.2 / speed;
            self.target_velocity.1 *= 0.2 / speed;
        }
        if speed < 0.05 {
            self.target_velocity.0 *= 0.05 / speed.max(0.001);
            self.target_velocity.1 *= 0.05 / speed.max(0.001);
        }
    }

    fn update_cursor(&mut self, dt: f32) {
        // In a real implementation, the cursor would be controlled by the decoder.
        // For calibration, we simulate smooth following with occasional perturbations.

        // Calculate direction to target
        let dx = self.target_pos.0 - self.cursor_pos.0;
        let dy = self.target_pos.1 - self.cursor_pos.1;

        // Move cursor toward target (simulating decoded intent)
        let follow_speed = 2.0; // How fast cursor follows
        self.cursor_pos.0 += dx * follow_speed * dt;
        self.cursor_pos.1 += dy * follow_speed * dt;

        // Apply perturbation if active
        if self.perturbation_active {
            // Push cursor away from target
            let perturb_strength = 0.3;
            let perturb_dx = (rand::random::<f32>() - 0.5) * perturb_strength;
            let perturb_dy = (rand::random::<f32>() - 0.5) * perturb_strength;
            self.cursor_pos.0 += perturb_dx * dt * 5.0;
            self.cursor_pos.1 += perturb_dy * dt * 5.0;
        }

        // Keep cursor in bounds
        self.cursor_pos.0 = self.cursor_pos.0.clamp(0.0, 1.0);
        self.cursor_pos.1 = self.cursor_pos.1.clamp(0.0, 1.0);
    }

    fn update_perturbations(&mut self) {
        // Check if it's time for a new perturbation
        if self.elapsed_time >= self.next_perturbation_time && !self.perturbation_active {
            self.perturbation_active = true;
            self.perturbation_end_time = self.elapsed_time + 0.3; // 300ms perturbation
            self.perturbation_count += 1;

            // Schedule next perturbation (random interval 2-5 seconds)
            self.next_perturbation_time = self.elapsed_time + 2.0 + rand::random::<f32>() * 3.0;
        }

        // Check if perturbation should end
        if self.perturbation_active && self.elapsed_time >= self.perturbation_end_time {
            self.perturbation_active = false;
        }
    }

    fn calculate_error(&mut self) {
        let dx = self.target_pos.0 - self.cursor_pos.0;
        let dy = self.target_pos.1 - self.cursor_pos.1;
        let error = (dx * dx + dy * dy).sqrt();

        self.total_error += error;
        self.error_samples += 1;
    }

    /// Renders the game and returns true when complete.
    pub fn show(&mut self, ctx: &egui::Context) -> bool {
        let dt = ctx.input(|i| i.stable_dt);

        match self.phase {
            GamePhase::Instructions => {
                self.show_instructions(ctx);
            }
            GamePhase::Tracking => {
                // Update game state
                self.elapsed_time += dt;
                self.update_target(dt);
                self.update_cursor(dt);
                self.update_perturbations();
                self.calculate_error();

                // Check for completion
                if self.elapsed_time >= self.game_duration {
                    self.phase = GamePhase::Complete;
                }

                self.show_tracking(ctx);
            }
            GamePhase::Complete => {
                self.show_complete(ctx);
            }
        }

        self.phase == GamePhase::Complete
    }

    fn show_instructions(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);

                ui.heading("Target Tracking");
                ui.add_space(20.0);

                ui.label("Try to keep the cursor (blue) on the target (green).");
                ui.label("The target will move around the screen.");
                ui.label("Sometimes the cursor will be pushed off track - that's intentional!");

                ui.add_space(30.0);

                if action_button(ui, "Start", true, ButtonVariant::Default) {
                    self.phase = GamePhase::Tracking;
                }
            });
        });
    }

    fn show_tracking(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Status bar
            ui.horizontal(|ui| {
                let remaining = (self.game_duration - self.elapsed_time).max(0.0);
                ui.label(format!("Time: {:.0}s", remaining));
                ui.separator();
                ui.label(format!("Perturbations: {}", self.perturbation_count));
                ui.separator();
                let avg_error = if self.error_samples > 0 {
                    self.total_error / self.error_samples as f32
                } else {
                    0.0
                };
                ui.label(format!("Avg Error: {:.2}", avg_error));

                if self.perturbation_active {
                    ui.separator();
                    ui.colored_label(egui::Color32::YELLOW, "⚠ PERTURBATION");
                }
            });

            ui.add_space(10.0);

            // Progress bar
            let progress_pct = (self.elapsed_time / self.game_duration).clamp(0.0, 1.0) * 100.0;
            ui.label(format!("{progress_pct:.0}%"));
            let _ = Progress::new(progress_pct).show(ui, &ui.ctx().armas_theme());

            ui.add_space(10.0);

            // Draw tracking area
            self.draw_tracking_area(ui);
        });
    }

    fn draw_tracking_area(&self, ui: &mut egui::Ui) {
        let available_size = ui.available_size();
        let area_size = available_size.x.min(available_size.y - 50.0).min(600.0);

        let (response, painter) =
            ui.allocate_painter(egui::vec2(area_size, area_size), egui::Sense::hover());

        let rect = response.rect;

        // Background
        painter.rect_filled(rect, 8.0, egui::Color32::from_rgb(30, 30, 40));
        painter.rect_stroke(
            rect,
            8.0,
            egui::Stroke::new(2.0, egui::Color32::GRAY),
            egui::StrokeKind::Outside,
        );

        // Convert normalized positions to screen positions
        let target_screen = egui::pos2(
            rect.min.x + self.target_pos.0 * rect.width(),
            rect.min.y + self.target_pos.1 * rect.height(),
        );

        let cursor_screen = egui::pos2(
            rect.min.x + self.cursor_pos.0 * rect.width(),
            rect.min.y + self.cursor_pos.1 * rect.height(),
        );

        // Draw target
        let target_radius = 25.0;
        painter.circle_filled(
            target_screen,
            target_radius,
            egui::Color32::from_rgb(50, 200, 100),
        );
        painter.circle_stroke(
            target_screen,
            target_radius,
            egui::Stroke::new(2.0, egui::Color32::WHITE),
        );

        // Draw cursor
        let cursor_radius = 15.0;
        let cursor_color = if self.perturbation_active {
            egui::Color32::from_rgb(200, 100, 50) // Orange during perturbation
        } else {
            egui::Color32::from_rgb(50, 100, 200) // Blue normally
        };
        painter.circle_filled(cursor_screen, cursor_radius, cursor_color);
        painter.circle_stroke(
            cursor_screen,
            cursor_radius,
            egui::Stroke::new(2.0, egui::Color32::WHITE),
        );

        // Draw line connecting them (shows error)
        painter.line_segment(
            [target_screen, cursor_screen],
            egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 100),
            ),
        );
    }

    /// Returns (avg_tracking_error, perturbation_count) for quality assessment.
    pub fn quality_metrics(&self) -> (f32, u32) {
        let avg_error = if self.error_samples > 0 {
            self.total_error / self.error_samples as f32
        } else {
            0.0
        };
        (avg_error, self.perturbation_count)
    }

    fn show_complete(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);

                ui.heading("Tracking Complete!");
                ui.add_space(20.0);

                let avg_error = if self.error_samples > 0 {
                    self.total_error / self.error_samples as f32
                } else {
                    0.0
                };

                ui.label(format!("Total perturbations: {}", self.perturbation_count));
                ui.label(format!("Average tracking error: {:.3}", avg_error));

                ui.add_space(20.0);
                ui.label("ErrP data has been recorded for training.");
            });
        });
    }
}

impl Default for TargetTrackingGame {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Construction tests ---

    #[test]
    fn new_game_has_correct_defaults() {
        let game = TargetTrackingGame::new();
        assert_eq!(game.cursor_pos, (0.5, 0.5));
        assert_eq!(game.target_pos, (0.5, 0.5));
        assert_eq!(game.game_duration, 120.0);
        assert_eq!(game.elapsed_time, 0.0);
        assert_eq!(game.perturbation_count, 0);
        assert_eq!(game.total_error, 0.0);
        assert_eq!(game.error_samples, 0);
        assert_eq!(game.phase, GamePhase::Instructions);
        assert!(!game.perturbation_active);
    }

    #[test]
    fn default_matches_new() {
        let a = TargetTrackingGame::new();
        let b = TargetTrackingGame::default();
        assert_eq!(a.cursor_pos, b.cursor_pos);
        assert_eq!(a.game_duration, b.game_duration);
        assert_eq!(a.perturbation_count, b.perturbation_count);
    }

    // --- Target movement tests ---

    #[test]
    fn update_target_moves_position() {
        let mut game = TargetTrackingGame::new();
        let initial = game.target_pos;
        game.update_target(0.1);
        assert_ne!(game.target_pos, initial);
    }

    #[test]
    fn update_target_bounces_off_right_edge() {
        let mut game = TargetTrackingGame::new();
        game.target_pos = (0.91, 0.5);
        game.target_velocity = (0.2, 0.05);
        game.update_target(0.1);
        // Velocity x should have flipped sign
        assert!(game.target_velocity.0 < 0.0);
    }

    #[test]
    fn update_target_bounces_off_left_edge() {
        let mut game = TargetTrackingGame::new();
        game.target_pos = (0.09, 0.5);
        game.target_velocity = (-0.2, 0.05);
        game.update_target(0.1);
        assert!(game.target_velocity.0 > 0.0);
    }

    #[test]
    fn update_target_bounces_off_bottom_edge() {
        let mut game = TargetTrackingGame::new();
        game.target_pos = (0.5, 0.91);
        game.target_velocity = (0.05, 0.2);
        game.update_target(0.1);
        assert!(game.target_velocity.1 < 0.0);
    }

    #[test]
    fn update_target_bounces_off_top_edge() {
        let mut game = TargetTrackingGame::new();
        game.target_pos = (0.5, 0.09);
        game.target_velocity = (0.05, -0.2);
        game.update_target(0.1);
        assert!(game.target_velocity.1 > 0.0);
    }

    #[test]
    fn update_target_clamps_speed_upper_bound() {
        let mut game = TargetTrackingGame::new();
        game.target_velocity = (0.5, 0.5);
        game.update_target(0.01);
        let speed = (game.target_velocity.0.powi(2) + game.target_velocity.1.powi(2)).sqrt();
        assert!(speed <= 0.21, "speed {speed} exceeds max");
    }

    // --- Cursor movement tests ---

    #[test]
    fn update_cursor_moves_toward_target() {
        let mut game = TargetTrackingGame::new();
        game.cursor_pos = (0.2, 0.2);
        game.target_pos = (0.8, 0.8);
        game.perturbation_active = false;
        game.update_cursor(0.1);
        // Cursor should have moved closer to target
        assert!(game.cursor_pos.0 > 0.2);
        assert!(game.cursor_pos.1 > 0.2);
    }

    #[test]
    fn update_cursor_clamps_to_bounds() {
        let mut game = TargetTrackingGame::new();
        game.cursor_pos = (0.99, 0.99);
        game.target_pos = (1.5, 1.5); // well outside
        game.perturbation_active = false;
        game.update_cursor(1.0);
        assert!(game.cursor_pos.0 <= 1.0);
        assert!(game.cursor_pos.1 <= 1.0);
    }

    #[test]
    fn update_cursor_clamps_lower_bound() {
        let mut game = TargetTrackingGame::new();
        game.cursor_pos = (0.01, 0.01);
        game.target_pos = (-0.5, -0.5);
        game.perturbation_active = false;
        game.update_cursor(1.0);
        assert!(game.cursor_pos.0 >= 0.0);
        assert!(game.cursor_pos.1 >= 0.0);
    }

    // --- Perturbation tests ---

    #[test]
    fn update_perturbations_triggers_at_scheduled_time() {
        let mut game = TargetTrackingGame::new();
        game.elapsed_time = 3.0; // equals next_perturbation_time
        game.update_perturbations();
        assert!(game.perturbation_active);
        assert_eq!(game.perturbation_count, 1);
    }

    #[test]
    fn update_perturbations_does_not_trigger_early() {
        let mut game = TargetTrackingGame::new();
        game.elapsed_time = 2.0; // before next_perturbation_time (3.0)
        game.update_perturbations();
        assert!(!game.perturbation_active);
        assert_eq!(game.perturbation_count, 0);
    }

    #[test]
    fn update_perturbations_ends_after_duration() {
        let mut game = TargetTrackingGame::new();
        // Trigger perturbation
        game.elapsed_time = 3.0;
        game.update_perturbations();
        assert!(game.perturbation_active);

        // Advance past perturbation end time (0.3s duration)
        game.elapsed_time = 3.4;
        game.update_perturbations();
        assert!(!game.perturbation_active);
    }

    #[test]
    fn update_perturbations_does_not_double_trigger() {
        let mut game = TargetTrackingGame::new();
        game.elapsed_time = 3.0;
        game.update_perturbations();
        assert_eq!(game.perturbation_count, 1);

        // Still active — should not trigger again
        game.elapsed_time = 3.1;
        game.update_perturbations();
        assert_eq!(game.perturbation_count, 1);
    }

    #[test]
    fn update_perturbations_schedules_next() {
        let mut game = TargetTrackingGame::new();
        game.elapsed_time = 3.0;
        game.update_perturbations();
        // next_perturbation_time should be 2-5s after current time
        assert!(game.next_perturbation_time >= 5.0); // 3.0 + 2.0
        assert!(game.next_perturbation_time <= 8.0); // 3.0 + 5.0
    }

    // --- Error tracking tests ---

    #[test]
    fn calculate_error_accumulates_samples() {
        let mut game = TargetTrackingGame::new();
        game.cursor_pos = (0.5, 0.5);
        game.target_pos = (0.5, 0.5);
        game.calculate_error();
        assert_eq!(game.error_samples, 1);
        assert_eq!(game.total_error, 0.0);
    }

    #[test]
    fn calculate_error_measures_distance() {
        let mut game = TargetTrackingGame::new();
        game.cursor_pos = (0.0, 0.0);
        game.target_pos = (0.3, 0.4);
        game.calculate_error();
        let expected = (0.3_f32.powi(2) + 0.4_f32.powi(2)).sqrt(); // 0.5
        assert!((game.total_error - expected).abs() < 1e-6);
    }

    #[test]
    fn calculate_error_accumulates_across_samples() {
        let mut game = TargetTrackingGame::new();
        game.cursor_pos = (0.0, 0.0);
        game.target_pos = (0.3, 0.4);
        game.calculate_error();
        game.calculate_error();
        assert_eq!(game.error_samples, 2);
        let expected = (0.3_f32.powi(2) + 0.4_f32.powi(2)).sqrt() * 2.0;
        assert!((game.total_error - expected).abs() < 1e-6);
    }

    // --- Quality metrics tests ---

    #[test]
    fn quality_metrics_zero_when_no_samples() {
        let game = TargetTrackingGame::new();
        let (avg_error, count) = game.quality_metrics();
        assert_eq!(avg_error, 0.0);
        assert_eq!(count, 0);
    }

    #[test]
    fn quality_metrics_returns_average_error() {
        let mut game = TargetTrackingGame::new();
        game.total_error = 2.0;
        game.error_samples = 4;
        game.perturbation_count = 7;
        let (avg_error, count) = game.quality_metrics();
        assert!((avg_error - 0.5).abs() < 1e-6);
        assert_eq!(count, 7);
    }

    // --- Phase transition tests ---

    #[test]
    fn game_starts_in_instructions_phase() {
        let game = TargetTrackingGame::new();
        assert_eq!(game.phase, GamePhase::Instructions);
    }
}

fn action_button(ui: &mut egui::Ui, label: &str, enabled: bool, variant: ButtonVariant) -> bool {
    if !enabled {
        return ui.add_enabled(false, egui::Button::new(label)).clicked();
    }

    Button::new(label)
        .variant(variant)
        .size(ButtonSize::Small)
        .show(ui, &ui.ctx().armas_theme())
        .clicked()
}
