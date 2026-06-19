//! # Calibration Panel
//!
//! An embeddable calibration UI that renders into a host egui context.
//! Unlike the old `CalibrationApp`, this panel does NOT own a device or
//! tokio runtime — it receives signal quality and sample data from the
//! host application (the hub).

use armas::components::Progress;
use armas::prelude::{ArmasContextExt, Button, ButtonSize, ButtonVariant};
use eframe::egui;

use neurohid_types::profile::CalibrationStep;

use super::games::{GameKind, GridMazeGame, TargetTrackingGame};
use super::wizard::WizardState;

/// The result of rendering the calibration panel for one frame.
#[derive(Debug, Clone, PartialEq)]
pub enum CalibrationPanelResult {
    /// Calibration is still in progress.
    InProgress,
    /// Calibration completed successfully.
    Completed(CalibrationQuality),
    /// User cancelled calibration.
    Cancelled,
}

/// Quality metrics from a completed calibration session.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationQuality {
    /// Number of correct trials in grid maze
    pub correct_trials: u32,
    /// Number of error trials in grid maze
    pub error_trials: u32,
    /// Average tracking error from target tracking
    pub avg_tracking_error: f32,
    /// Number of perturbations applied
    pub perturbation_count: u32,
}

/// The current screen within the calibration panel.
#[derive(Debug, Clone, PartialEq)]
enum Screen {
    /// Initial welcome screen explaining the process
    Welcome,
    /// The step-by-step calibration wizard
    Wizard,
    /// Playing the grid maze game
    GridMaze,
    /// Playing the target tracking game
    TargetTracking,
    /// Calibration complete
    Complete,
}

/// An embeddable calibration panel that does NOT own a device or runtime.
///
/// The host application (hub) is responsible for:
/// - Starting the service in calibration mode
/// - Feeding signal quality via `set_signal_quality()`
/// - Calling `show()` each frame
pub struct CalibrationPanel {
    screen: Screen,
    wizard: WizardState,
    grid_maze: Option<GridMazeGame>,
    target_tracking: Option<TargetTrackingGame>,
    status_message: String,
    signal_quality: f32,
    /// Stored quality from completed grid maze
    maze_quality: Option<(u32, u32)>,
    /// Stored quality from completed target tracking
    tracking_quality: Option<(f32, u32)>,
    /// Start time for the decoder training phase.
    decoder_training_started_at: Option<std::time::Instant>,
    /// When set, only this game is run (wizard then that game then complete).
    game_kind: Option<GameKind>,
    /// User requested exit during game (checked at start of show(), then cleared).
    cancel_requested: bool,
}

impl CalibrationPanel {
    /// Creates a new calibration panel (full flow: wizard then both games).
    pub fn new() -> Self {
        Self {
            screen: Screen::Welcome,
            wizard: WizardState::new(),
            grid_maze: None,
            target_tracking: None,
            status_message: String::new(),
            signal_quality: 0.0,
            maze_quality: None,
            tracking_quality: None,
            decoder_training_started_at: None,
            game_kind: None,
            cancel_requested: false,
        }
    }

    /// Creates a calibration panel that runs only the given game (wizard steps
    /// for that game, then the game, then complete). Used when the hub game
    /// list is used and the user picks a single game.
    pub fn new_for_game(kind: GameKind) -> Self {
        Self {
            game_kind: Some(kind),
            ..Self::new()
        }
    }

    /// Feed signal quality from the service's shared state.
    pub fn set_signal_quality(&mut self, quality: f32) {
        self.signal_quality = quality;
    }

    /// Renders the calibration UI into the host egui context.
    /// Called each frame by the hub.
    pub fn show(&mut self, ctx: &egui::Context) -> CalibrationPanelResult {
        if self.cancel_requested {
            self.cancel_requested = false;
            return CalibrationPanelResult::Cancelled;
        }

        let mut result = CalibrationPanelResult::InProgress;

        match self.screen.clone() {
            Screen::Welcome => self.show_welcome(ctx, &mut result),
            Screen::Wizard => self.show_wizard(ctx),
            Screen::GridMaze => self.show_grid_maze(ctx),
            Screen::TargetTracking => self.show_target_tracking(ctx),
            Screen::Complete => {
                self.show_complete(ctx);
                result = CalibrationPanelResult::Completed(CalibrationQuality {
                    correct_trials: self.maze_quality.map(|(c, _)| c).unwrap_or(0),
                    error_trials: self.maze_quality.map(|(_, e)| e).unwrap_or(0),
                    avg_tracking_error: self.tracking_quality.map(|(e, _)| e).unwrap_or(0.0),
                    perturbation_count: self.tracking_quality.map(|(_, p)| p).unwrap_or(0),
                });
            }
        }

        // Request continuous repainting for smooth animations
        ctx.request_repaint();

        result
    }

    fn show_welcome(&mut self, ctx: &egui::Context, result: &mut CalibrationPanelResult) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);

                ui.heading("Calibration");
                ui.add_space(15.0);

                ui.label("This wizard will calibrate your brain-computer interface.");
                ui.label("The process takes about 15-30 minutes.");
                ui.add_space(20.0);

                // Signal quality display
                ui.group(|ui| {
                    ui.heading("Signal Status");
                    ui.add_space(8.0);

                    let quality_pct = self.signal_quality * 100.0;
                    ui.label(format!("Signal quality: {:.0}%", quality_pct));
                    let _ = Progress::new(quality_pct).show(ui, &ui.ctx().armas_theme());
                    let quality_text = if quality_pct > 70.0 {
                        "Status: good"
                    } else if quality_pct > 50.0 {
                        "Status: fair"
                    } else {
                        "Status: low"
                    };
                    ui.label(egui::RichText::new(quality_text).small());
                });

                ui.add_space(20.0);

                let can_start = self.signal_quality > 0.5;

                ui.horizontal(|ui| {
                    ui.add_enabled_ui(can_start, |ui| {
                        if action_button(ui, "Begin Calibration", true, ButtonVariant::Default) {
                            self.screen = Screen::Wizard;
                            self.wizard.start();
                        }
                    });

                    if action_button(ui, "Cancel", true, ButtonVariant::Secondary) {
                        *result = CalibrationPanelResult::Cancelled;
                    }
                });

                if !can_start {
                    ui.add_space(8.0);
                    ui.label("Signal quality too low. Please adjust headset placement.");
                }

                if !self.status_message.is_empty() {
                    ui.add_space(10.0);
                    ui.label(&self.status_message);
                }
            });
        });
    }

    fn show_wizard(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);

                // Progress indicator
                ui.horizontal(|ui| {
                    for step in 0..CalibrationStep::total_steps() {
                        let current = self.wizard.current_step().index();
                        let color = if step < current {
                            egui::Color32::GREEN
                        } else if step == current {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::GRAY
                        };

                        ui.colored_label(color, "●");
                    }
                });

                let current_step = self.wizard.current_step().index() + 1;
                let total_steps = CalibrationStep::total_steps();
                let progress_pct = (current_step as f32 / total_steps as f32) * 100.0;
                ui.add_space(8.0);
                ui.label(format!("Step {}/{}", current_step, total_steps));
                let _ = Progress::new(progress_pct).show(ui, &ui.ctx().armas_theme());

                ui.add_space(20.0);
                ui.heading(self.wizard.current_step().display_name());
                ui.add_space(20.0);

                // Step-specific content
                match self.wizard.current_step() {
                    CalibrationStep::SignalCheck => {
                        self.show_signal_check(ui);
                    }
                    CalibrationStep::ErrPDiscrete => {
                        self.show_errp_discrete_intro(ui);
                    }
                    CalibrationStep::ErrPContinuous => {
                        self.show_errp_continuous_intro(ui);
                    }
                    CalibrationStep::DecoderTraining => {
                        self.show_decoder_training(ui);
                    }
                    CalibrationStep::Validation => {
                        self.show_validation(ui);
                    }
                }
            });
        });
    }

    fn show_signal_check(&mut self, ui: &mut egui::Ui) {
        ui.label("Let's verify your signal quality before we begin.");
        ui.add_space(10.0);

        let quality_pct = self.signal_quality * 100.0;
        ui.label(format!("Current signal quality: {:.0}%", quality_pct));
        let _ = Progress::new(quality_pct).show(ui, &ui.ctx().armas_theme());

        let quality_good = self.signal_quality > 0.7;

        ui.add_space(20.0);

        if quality_good {
            ui.colored_label(egui::Color32::GREEN, "Signal quality is good!");
            ui.add_space(10.0);
            if action_button(ui, "Continue", true, ButtonVariant::Default) {
                self.wizard.advance();
            }
        } else {
            ui.colored_label(egui::Color32::YELLOW, "Signal quality needs improvement.");
            ui.label("Tips:");
            ui.label("  Ensure all sensors have good contact");
            ui.label("  Moisten the sensors slightly if dry");
            ui.label("  Stay still and relax");
        }
    }

    fn show_errp_discrete_intro(&mut self, ui: &mut egui::Ui) {
        if self.game_kind == Some(GameKind::TargetTracking) {
            ui.label("This session will run Target Tracking only.");
            ui.label("Continuing to the Target Tracking intro.");
            ui.add_space(20.0);
            if action_button(ui, "Continue", true, ButtonVariant::Default) {
                self.wizard.advance();
            }
            return;
        }

        ui.label("In this game, you'll navigate a maze using thought commands.");
        ui.label("Sometimes the system will deliberately make mistakes.");
        ui.label("This helps us learn what your brain signals look like when errors occur.");

        ui.add_space(20.0);

        if action_button(ui, "Start Grid Maze", true, ButtonVariant::Default) {
            self.grid_maze = Some(GridMazeGame::new());
            self.screen = Screen::GridMaze;
        }
    }

    fn show_errp_continuous_intro(&mut self, ui: &mut egui::Ui) {
        ui.label("Now we'll calibrate for continuous control.");
        ui.label("Try to keep the cursor on the moving target.");
        ui.label("The system will occasionally push the cursor off track.");

        ui.add_space(20.0);

        if action_button(ui, "Start Target Tracking", true, ButtonVariant::Default) {
            self.target_tracking = Some(TargetTrackingGame::new());
            self.screen = Screen::TargetTracking;
        }
    }

    fn show_decoder_training(&mut self, ui: &mut egui::Ui) {
        let started = self
            .decoder_training_started_at
            .get_or_insert_with(std::time::Instant::now);
        let progress = (started.elapsed().as_secs_f32() / 4.0).clamp(0.0, 1.0);

        ui.label("Training your personalized decoder...");
        ui.add_space(10.0);
        ui.label(format!("Training... {:.0}%", progress * 100.0));
        let _ = Progress::new(progress * 100.0).show(ui, &ui.ctx().armas_theme());
        ui.add_space(12.0);

        if progress < 1.0 {
            ui.label("Collecting calibration summary and preparing initial model metadata.");
            return;
        }

        ui.colored_label(
            egui::Color32::YELLOW,
            "Calibration summary prepared; validated model artifact not confirmed yet.",
        );
        if action_button(ui, "Continue", true, ButtonVariant::Default) {
            self.decoder_training_started_at = None;
            self.wizard.advance();
        }
    }

    fn show_validation(&mut self, ui: &mut egui::Ui) {
        ui.label("Let's do a quick test to make sure everything works!");
        ui.add_space(20.0);

        if action_button(ui, "Complete Calibration", true, ButtonVariant::Default) {
            self.screen = Screen::Complete;
        }
    }

    fn show_grid_maze(&mut self, ctx: &egui::Context) {
        self.show_game_exit_button(ctx);

        let game_complete = if let Some(game) = &mut self.grid_maze {
            game.show(ctx)
        } else {
            false
        };

        if game_complete {
            if let Some(game) = &self.grid_maze {
                self.maze_quality = Some(game.quality_metrics());
            }
            if self.game_kind == Some(GameKind::GridMaze) {
                self.screen = Screen::Complete;
            } else {
                self.wizard.advance();
                self.screen = Screen::Wizard;
            }
        }
    }

    fn show_target_tracking(&mut self, ctx: &egui::Context) {
        self.show_game_exit_button(ctx);

        let game_complete = if let Some(game) = &mut self.target_tracking {
            game.show(ctx)
        } else {
            false
        };

        if game_complete {
            if let Some(game) = &self.target_tracking {
                self.tracking_quality = Some(game.quality_metrics());
            }
            if self.game_kind == Some(GameKind::TargetTracking) {
                self.screen = Screen::Complete;
            } else {
                self.wizard.advance();
                self.screen = Screen::Wizard;
            }
        }
    }

    /// Draws an "Exit calibration" button in a top bar when playing a game so the user can cancel mid-game.
    fn show_game_exit_button(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("calibration_exit_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if action_button(ui, "Exit calibration", true, ButtonVariant::Secondary) {
                        self.cancel_requested = true;
                    }
                });
            });
        });
    }

    fn show_complete(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);

                ui.heading("Calibration Data Collected");
                ui.add_space(20.0);

                ui.label("Calibration tasks are complete for this session.");
                ui.label("A validated decoder model must be confirmed before claiming readiness.");
                ui.add_space(10.0);
                ui.label("The service will now resume normal operation.");
            });
        });
    }
}

impl Default for CalibrationPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_panel_starts_at_welcome_screen() {
        let panel = CalibrationPanel::new();
        assert_eq!(panel.screen, Screen::Welcome);
    }

    #[test]
    fn new_panel_has_zero_signal_quality() {
        let panel = CalibrationPanel::new();
        assert_eq!(panel.signal_quality, 0.0);
    }

    #[test]
    fn new_panel_has_no_game_instances() {
        let panel = CalibrationPanel::new();
        assert!(panel.grid_maze.is_none());
        assert!(panel.target_tracking.is_none());
    }

    #[test]
    fn new_panel_has_no_quality_data() {
        let panel = CalibrationPanel::new();
        assert!(panel.maze_quality.is_none());
        assert!(panel.tracking_quality.is_none());
    }

    #[test]
    fn default_matches_new() {
        let a = CalibrationPanel::new();
        let b = CalibrationPanel::default();
        assert_eq!(a.screen, b.screen);
        assert_eq!(a.signal_quality, b.signal_quality);
    }

    #[test]
    fn set_signal_quality_updates_value() {
        let mut panel = CalibrationPanel::new();
        panel.set_signal_quality(0.85);
        assert!((panel.signal_quality - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn set_signal_quality_can_be_zero() {
        let mut panel = CalibrationPanel::new();
        panel.set_signal_quality(0.75);
        panel.set_signal_quality(0.0);
        assert_eq!(panel.signal_quality, 0.0);
    }

    #[test]
    fn calibration_quality_struct_fields() {
        let quality = CalibrationQuality {
            correct_trials: 35,
            error_trials: 15,
            avg_tracking_error: 0.042,
            perturbation_count: 20,
        };
        assert_eq!(quality.correct_trials, 35);
        assert_eq!(quality.error_trials, 15);
        assert!((quality.avg_tracking_error - 0.042).abs() < 1e-6);
        assert_eq!(quality.perturbation_count, 20);
    }

    #[test]
    fn calibration_quality_clone() {
        let quality = CalibrationQuality {
            correct_trials: 10,
            error_trials: 5,
            avg_tracking_error: 0.1,
            perturbation_count: 3,
        };
        let cloned = quality.clone();
        assert_eq!(quality, cloned);
    }

    #[test]
    fn calibration_panel_result_in_progress() {
        let result = CalibrationPanelResult::InProgress;
        assert_eq!(result, CalibrationPanelResult::InProgress);
    }

    #[test]
    fn calibration_panel_result_cancelled() {
        let result = CalibrationPanelResult::Cancelled;
        assert_eq!(result, CalibrationPanelResult::Cancelled);
    }

    #[test]
    fn calibration_panel_result_completed_carries_quality() {
        let quality = CalibrationQuality {
            correct_trials: 35,
            error_trials: 15,
            avg_tracking_error: 0.05,
            perturbation_count: 20,
        };
        let result = CalibrationPanelResult::Completed(quality.clone());
        assert_eq!(result, CalibrationPanelResult::Completed(quality));
    }

    #[test]
    fn calibration_panel_result_variants_not_equal() {
        assert_ne!(
            CalibrationPanelResult::InProgress,
            CalibrationPanelResult::Cancelled
        );
    }

    #[test]
    fn screen_variants_are_distinct() {
        assert_ne!(Screen::Welcome, Screen::Wizard);
        assert_ne!(Screen::Wizard, Screen::GridMaze);
        assert_ne!(Screen::GridMaze, Screen::TargetTracking);
        assert_ne!(Screen::TargetTracking, Screen::Complete);
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
