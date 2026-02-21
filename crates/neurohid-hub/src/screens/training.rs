//! # Training Screen
//!
//! Decoder training configuration and live progress. Split layout: config/setup
//! pane (model path, params, profile, trigger) and live progress/metrics pane
//! from ControlSnapshot and TrainerSnapshot.

use std::time::{Duration, Instant};

use eframe::egui;

use neurohid_types::config::DecoderConfig;

use crate::service_manager::ServiceManager;
use crate::state::HubState;
use crate::theme;

const TRAINER_SNAPSHOT_POLL_INTERVAL: Duration = Duration::from_millis(1000);

pub struct TrainingScreen {
    last_trainer_snapshot_poll: Option<Instant>,
}

impl Default for TrainingScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl TrainingScreen {
    pub fn new() -> Self {
        Self {
            last_trainer_snapshot_poll: None,
        }
    }

    /// Renders the training screen: config/setup pane and live progress/metrics pane.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &HubState,
        service_manager: &mut ServiceManager,
    ) {
        theme::page_header(
            ui,
            "Training",
            "Configure and run decoder training; view progress and metrics",
        );

        let snap = &state.service_snapshot;
        let trainer_snapshot = self.maybe_poll_trainer_snapshot(service_manager, snap.running);

        // Split layout: config (top) | live progress (bottom)
        self.show_config_pane(ui, state);
        ui.add_space(8.0);
        self.show_live_progress_pane(ui, state, snap, trainer_snapshot.as_ref());
    }

    fn show_config_pane(&mut self, ui: &mut egui::Ui, state: &HubState) {
        theme::card_frame(ui).show(ui, |ui| {
            ui.heading("Setup");
            ui.add_space(6.0);

            let decoder = &state.config.decoder;
            ui.label(
                egui::RichText::new("Model path (from config)")
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.monospace(&decoder.model_path);
            ui.add_space(8.0);

            ui.label(
                egui::RichText::new("Active profile / dataset")
                    .small()
                    .color(egui::Color32::GRAY),
            );
            let profile_label = state
                .active_profile_id
                .as_ref()
                .map(|id| id.0.as_str())
                .unwrap_or("None");
            let profile_name = state
                .profiles
                .iter()
                .find(|p| state.active_profile_id.as_ref() == Some(&p.id))
                .map(|p| p.name.as_str())
                .unwrap_or(profile_label);
            ui.label(profile_name);
            if state.active_profile_id.is_none() {
                theme::status_chip(
                    ui,
                    "Select a profile in Profiles or Dashboard",
                    theme::Intent::Muted,
                );
            }
            ui.add_space(8.0);

            self.show_decoder_params(ui, decoder);
            ui.add_space(12.0);

            let can_trigger = state.service_snapshot.running && state.active_profile_id.is_some();
            let trigger_clicked = theme::action_button(
                ui,
                "Train on collected data",
                can_trigger,
                theme::ButtonTone::Primary,
            );
            if trigger_clicked {
                // Stub: control protocol does not yet expose a dedicated "start training"
                // command. Training is started by calibration or via Dashboard "Train + Stage
                // Candidate". Follow-up: add ControlCommand::StartTraining or wire same
                // train-stage job from this screen.
            }
            if !state.service_snapshot.running {
                theme::status_chip(
                    ui,
                    "Start the service from Dashboard first",
                    theme::Intent::Muted,
                );
            } else if state.active_profile_id.is_none() {
                theme::status_chip(
                    ui,
                    "Select an active profile to train on its data",
                    theme::Intent::Muted,
                );
            } else {
                ui.label(
                    egui::RichText::new(
                        "Use Dashboard → Train + Stage Candidate to train on session data. \
                         Direct trigger from this screen is planned.",
                    )
                    .small()
                    .color(egui::Color32::GRAY),
                );
            }
        });
    }

    fn show_decoder_params(&self, ui: &mut egui::Ui, decoder: &DecoderConfig) {
        ui.label(
            egui::RichText::new("Training parameters (from config)")
                .small()
                .color(egui::Color32::GRAY),
        );
        ui.horizontal_wrapped(|ui| {
            ui.label("learning_rate:");
            ui.monospace(format!("{:.2e}", decoder.learning_rate));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("gamma:");
            ui.monospace(format!("{}", decoder.gamma));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("batch_size:");
            ui.monospace(format!("{}", decoder.batch_size));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("update_frequency_steps:");
            ui.monospace(format!("{}", decoder.update_frequency_steps));
        });
    }

    fn show_live_progress_pane(
        &self,
        ui: &mut egui::Ui,
        state: &HubState,
        snap: &crate::state::ServiceSnapshot,
        trainer_snapshot: Option<&neurohid_types::control::TrainerSnapshot>,
    ) {
        theme::card_frame(ui).show(ui, |ui| {
            ui.heading("Live progress & metrics");
            ui.add_space(6.0);

            if !snap.running {
                theme::status_chip(
                    ui,
                    "Start the service from Dashboard to see training progress",
                    theme::Intent::Muted,
                );
                return;
            }

            if let Some(trainer) = trainer_snapshot {
                let intent = if trainer.trainer_connected {
                    theme::Intent::Success
                } else {
                    theme::Intent::Warning
                };
                theme::status_chip(ui, &format!("Trainer: {}", trainer.trainer_state), intent);
                ui.add_space(4.0);
            }

            let has_metrics = snap.trainer_step.is_some()
                || snap.trainer_policy_loss.is_some()
                || snap.trainer_value_loss.is_some()
                || snap.trainer_entropy.is_some()
                || snap.trainer_replay_size.is_some();

            if !has_metrics && trainer_snapshot.is_none() {
                theme::status_chip(
                    ui,
                    "Waiting for trainer connection and metrics…",
                    theme::Intent::Info,
                );
                return;
            }

            ui.horizontal_wrapped(|ui| {
                if let Some(v) = snap.trainer_replay_size {
                    ui.label("Replay size:");
                    ui.monospace(format!("{}", v));
                    ui.add_space(8.0);
                }
                if let Some(v) = snap.trainer_step {
                    ui.label("Step:");
                    ui.monospace(format!("{}", v));
                    ui.add_space(8.0);
                }
            });

            ui.horizontal_wrapped(|ui| {
                if let Some(v) = snap.trainer_policy_loss {
                    ui.label("Policy loss:");
                    ui.monospace(format!("{:.4}", v));
                    ui.add_space(8.0);
                }
                if let Some(v) = snap.trainer_value_loss {
                    ui.label("Value loss:");
                    ui.monospace(format!("{:.4}", v));
                    ui.add_space(8.0);
                }
                if let Some(v) = snap.trainer_entropy {
                    ui.label("Entropy:");
                    ui.monospace(format!("{:.4}", v));
                }
            });

            if let Some(err) = &snap.trainer_last_error {
                ui.add_space(6.0);
                theme::status_chip(ui, err, theme::Intent::Danger);
            }

            if state.service_snapshot.ml_bridge_stalled {
                ui.add_space(4.0);
                theme::status_chip(ui, "ML bridge stalled", theme::Intent::Warning);
            }
        });
    }

    fn maybe_poll_trainer_snapshot(
        &mut self,
        service_manager: &mut ServiceManager,
        runtime_running: bool,
    ) -> Option<neurohid_types::control::TrainerSnapshot> {
        if !runtime_running {
            self.last_trainer_snapshot_poll = None;
            return None;
        }
        let now = Instant::now();
        if self
            .last_trainer_snapshot_poll
            .is_some_and(|t| now.duration_since(t) < TRAINER_SNAPSHOT_POLL_INTERVAL)
        {
            return None;
        }
        self.last_trainer_snapshot_poll = Some(now);
        service_manager.trainer_snapshot()
    }
}
