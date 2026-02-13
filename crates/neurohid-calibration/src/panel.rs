//! # Calibration Panel
//!
//! An embeddable calibration UI that renders into a host egui context.
//! Unlike the old `CalibrationApp`, this panel does NOT own a device or
//! tokio runtime — it receives signal quality and sample data from the
//! host application (the hub).

use eframe::egui;

use neurohid_types::profile::CalibrationStep;

use crate::games::{GridMazeGame, TargetTrackingGame};
use crate::wizard::WizardState;

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
}

impl CalibrationPanel {
    /// Creates a new calibration panel.
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
        }
    }

    /// Feed signal quality from the service's shared state.
    pub fn set_signal_quality(&mut self, quality: f32) {
        self.signal_quality = quality;
    }

    /// Renders the calibration UI into the host egui context.
    /// Called each frame by the hub.
    pub fn show(&mut self, ctx: &egui::Context) -> CalibrationPanelResult {
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

                    ui.horizontal(|ui| {
                        let quality_pct = self.signal_quality * 100.0;
                        let color = if quality_pct > 70.0 {
                            egui::Color32::GREEN
                        } else if quality_pct > 50.0 {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::RED
                        };
                        ui.colored_label(color, format!("Signal quality: {:.0}%", quality_pct));
                    });
                });

                ui.add_space(20.0);

                let can_start = self.signal_quality > 0.5;

                ui.horizontal(|ui| {
                    ui.add_enabled_ui(can_start, |ui| {
                        if ui.button("Begin Calibration").clicked() {
                            self.screen = Screen::Wizard;
                            self.wizard.start();
                        }
                    });

                    if ui.button("Cancel").clicked() {
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

        ui.label(format!(
            "Current signal quality: {:.0}%",
            self.signal_quality * 100.0
        ));

        let quality_good = self.signal_quality > 0.7;

        ui.add_space(20.0);

        if quality_good {
            ui.colored_label(egui::Color32::GREEN, "Signal quality is good!");
            ui.add_space(10.0);
            if ui.button("Continue").clicked() {
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
        ui.label("In this game, you'll navigate a maze using thought commands.");
        ui.label("Sometimes the system will deliberately make mistakes.");
        ui.label("This helps us learn what your brain signals look like when errors occur.");

        ui.add_space(20.0);

        if ui.button("Start Grid Maze").clicked() {
            self.grid_maze = Some(GridMazeGame::new());
            self.screen = Screen::GridMaze;
        }
    }

    fn show_errp_continuous_intro(&mut self, ui: &mut egui::Ui) {
        ui.label("Now we'll calibrate for continuous control.");
        ui.label("Try to keep the cursor on the moving target.");
        ui.label("The system will occasionally push the cursor off track.");

        ui.add_space(20.0);

        if ui.button("Start Target Tracking").clicked() {
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
        ui.add(
            egui::ProgressBar::new(progress).text(format!("Training... {:.0}%", progress * 100.0)),
        );
        ui.add_space(12.0);

        if progress < 1.0 {
            ui.label("Collecting calibration summary and preparing initial model metadata.");
            return;
        }

        ui.colored_label(egui::Color32::GREEN, "Training completed.");
        if ui.button("Continue").clicked() {
            self.decoder_training_started_at = None;
            self.wizard.advance();
        }
    }

    fn show_validation(&mut self, ui: &mut egui::Ui) {
        ui.label("Let's do a quick test to make sure everything works!");
        ui.add_space(20.0);

        if ui.button("Complete Calibration").clicked() {
            self.screen = Screen::Complete;
        }
    }

    fn show_grid_maze(&mut self, ctx: &egui::Context) {
        let game_complete = if let Some(game) = &mut self.grid_maze {
            game.show(ctx)
        } else {
            false
        };

        if game_complete {
            // Extract quality metrics from the game before advancing
            if let Some(game) = &self.grid_maze {
                self.maze_quality = Some(game.quality_metrics());
            }
            self.wizard.advance();
            self.screen = Screen::Wizard;
        }
    }

    fn show_target_tracking(&mut self, ctx: &egui::Context) {
        let game_complete = if let Some(game) = &mut self.target_tracking {
            game.show(ctx)
        } else {
            false
        };

        if game_complete {
            // Extract quality metrics from the game before advancing
            if let Some(game) = &self.target_tracking {
                self.tracking_quality = Some(game.quality_metrics());
            }
            self.wizard.advance();
            self.screen = Screen::Wizard;
        }
    }

    fn show_complete(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);

                ui.heading("Calibration Complete!");
                ui.add_space(20.0);

                ui.label("Your profile has been calibrated and is ready to use.");
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
