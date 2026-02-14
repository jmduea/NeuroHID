//! # Calibration Screen
//!
//! Wraps `neurohid_calibration::CalibrationPanel` and manages the
//! calibration lifecycle within the hub. Enters/exits calibration mode
//! on the service so that HID emission pauses during calibration.

use eframe::egui;
use serde::Serialize;

use neurohid_calibration::panel::{CalibrationPanel, CalibrationPanelResult};
use neurohid_types::model::{
    ModelManifest, NormalizationStats, CURRENT_ACTION_SCHEMA_VERSION,
    CURRENT_FEATURE_SCHEMA_VERSION,
};
use neurohid_types::profile::{CalibrationQuality, CalibrationState};

use crate::service_manager::ServiceManager;
use crate::state::HubState;

pub struct CalibrationScreen {
    panel: Option<CalibrationPanel>,
}

#[derive(Debug, Serialize)]
struct CalibrationArtifact {
    completed_at: i64,
    correct_trials: u32,
    error_trials: u32,
    avg_tracking_error: f32,
    perturbation_count: u32,
}

impl Default for CalibrationScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl CalibrationScreen {
    pub fn new() -> Self {
        Self { panel: None }
    }

    /// Whether the calibration panel is currently active (games running).
    pub fn is_panel_active(&self) -> bool {
        self.panel.is_some()
    }

    /// Render the active calibration panel directly into the remaining
    /// egui context space. Called instead of the hub's CentralPanel to
    /// avoid conflicting panel layouts.
    pub fn show_active_panel(
        &mut self,
        state: &mut HubState,
        service_manager: &mut ServiceManager,
        runtime: &tokio::runtime::Runtime,
        ctx: &egui::Context,
    ) {
        let panel = match &mut self.panel {
            Some(p) => p,
            None => return,
        };

        // Feed signal quality from the service
        panel.set_signal_quality(state.service_snapshot.signal_quality);

        let result = panel.show(ctx);

        match result {
            CalibrationPanelResult::InProgress => {}
            CalibrationPanelResult::Completed(quality) => {
                let profile_ready = self.persist_calibration_outputs(state, runtime, &quality);
                service_manager.exit_calibration_mode();
                self.panel = None;
                state.refresh_profiles(runtime);
                if let Some(profile_id) = state.active_profile_id.clone() {
                    let profile_name = state
                        .profiles
                        .iter()
                        .find(|p| p.id == profile_id)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| profile_id.to_string());
                    service_manager.set_active_profile(
                        Some(profile_id),
                        profile_name,
                        profile_ready,
                    );
                }
                tracing::info!("Calibration completed");
            }
            CalibrationPanelResult::Cancelled => {
                service_manager.exit_calibration_mode();
                self.panel = None;
                tracing::info!("Calibration cancelled");
            }
        }
    }

    /// Render the calibration entry screen (before games start).
    /// This renders into the hub's existing CentralPanel.
    pub fn show_entry(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut HubState,
        service_manager: &mut ServiceManager,
    ) {
        let snap = &state.service_snapshot;

        ui.label(
            egui::RichText::new("Calibration")
                .text_style(egui::TextStyle::Heading)
                .color(egui::Color32::from_rgb(225, 233, 245)),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Calibrate your brain-computer interface by playing interactive games.")
                .small()
                .color(egui::Color32::from_rgb(128, 145, 167)),
        );
        ui.label(
            egui::RichText::new("This process trains the ErrP detector and initial decoder model.")
                .small()
                .color(egui::Color32::from_rgb(128, 145, 167)),
        );
        ui.add_space(16.0);

        if !snap.running {
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(20, 25, 34))
                .show(ui, |ui| {
                ui.colored_label(egui::Color32::YELLOW, "Service is not running");
                ui.label("Start the service from the Dashboard before calibrating.");
                ui.label("The service provides the device connection needed for calibration.");
                });
            return;
        }

        if !snap.device_connected {
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(20, 25, 34))
                .show(ui, |ui| {
                ui.colored_label(egui::Color32::YELLOW, "No device connected");
                ui.label("Wait for the device to connect, then start calibration.");
                });
            return;
        }

        // Show current calibration status
        if let Some(profile_id) = &state.active_profile_id {
            let profile = state.profiles.iter().find(|p| &p.id == profile_id);
            if let Some(profile) = profile {
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_rgb(20, 25, 34))
                    .show(ui, |ui| {
                    ui.label(format!("Active profile: {}", profile.name));
                    let cal_status = match &profile.calibration_state {
                        CalibrationState::NotCalibrated => "Not calibrated",
                        CalibrationState::InProgress { .. } => "In progress",
                        CalibrationState::CompletedGood { .. } => "Good",
                        CalibrationState::CompletedAcceptable { .. } => "Acceptable",
                        CalibrationState::CompletedPoor { .. } => "Poor",
                        CalibrationState::NeedsRecalibration { .. } => "Needs recalibration",
                    };
                    ui.label(format!("Calibration: {}", cal_status));
                    });
            }
        }

        ui.add_space(16.0);

        if ui.button("Start Calibration").clicked() {
            service_manager.enter_calibration_mode();

            let mut panel = CalibrationPanel::new();
            panel.set_signal_quality(snap.signal_quality);
            self.panel = Some(panel);

            tracing::info!("Starting calibration session");
        }
    }

    fn persist_calibration_outputs(
        &self,
        state: &mut HubState,
        runtime: &tokio::runtime::Runtime,
        quality: &neurohid_calibration::panel::CalibrationQuality,
    ) -> bool {
        let Some(profile_id) = state.active_profile_id.clone() else {
            tracing::warn!("Calibration completed without an active profile; skipping persistence");
            return false;
        };

        let completed_at = neurohid_types::now_micros();
        let calibration_quality = to_profile_quality(quality);
        let calibration_state = calibration_quality.to_state(completed_at);

        let mut metadata = match runtime.block_on(state.profile_store.get_metadata(&profile_id)) {
            Ok(metadata) => metadata,
            Err(e) => {
                tracing::error!(
                    "Failed to load profile metadata after calibration for {}: {}",
                    profile_id,
                    e
                );
                return false;
            }
        };

        metadata.calibration_state = calibration_state;
        metadata.last_calibrated_at = Some(completed_at);
        let profile_ready = metadata.calibration_state.is_ready();

        if let Err(e) = runtime.block_on(state.profile_store.save_metadata(&metadata)) {
            tracing::error!(
                "Failed to persist profile metadata after calibration for {}: {}",
                profile_id,
                e
            );
        }

        let artifact = CalibrationArtifact {
            completed_at,
            correct_trials: quality.correct_trials,
            error_trials: quality.error_trials,
            avg_tracking_error: quality.avg_tracking_error,
            perturbation_count: quality.perturbation_count,
        };

        match serde_json::to_vec(&artifact) {
            Ok(payload) => {
                if let Err(e) =
                    runtime.block_on(state.profile_store.save_calibration(&profile_id, &payload))
                {
                    tracing::error!(
                        "Failed to persist calibration artifact for {}: {}",
                        profile_id,
                        e
                    );
                }
            }
            Err(e) => {
                tracing::error!(
                    "Failed to encode calibration artifact for {}: {}",
                    profile_id,
                    e
                );
            }
        }

        // Seed a bootstrap manifest so Rust inference loaders have schema metadata
        // even before a full trainer-produced ONNX artifact is available.
        let manifest = ModelManifest {
            model_version: "bootstrap-0".to_string(),
            input_dim: 180,
            feature_schema_version: CURRENT_FEATURE_SCHEMA_VERSION,
            action_schema_version: CURRENT_ACTION_SCHEMA_VERSION,
            normalization_stats: NormalizationStats {
                mean: vec![0.0; 180],
                std: vec![1.0; 180],
            },
            trained_at: completed_at,
        };
        if let Err(e) = runtime.block_on(
            state
                .profile_store
                .save_decoder_manifest(&profile_id, &manifest),
        ) {
            tracing::warn!("Failed to save decoder manifest for {}: {}", profile_id, e);
        }
        profile_ready
    }
}

fn to_profile_quality(
    quality: &neurohid_calibration::panel::CalibrationQuality,
) -> CalibrationQuality {
    let trial_count = quality.correct_trials + quality.error_trials;
    let errp_accuracy = if trial_count > 0 {
        quality.correct_trials as f32 / trial_count as f32
    } else {
        0.0
    };

    // Use observed error trials as a proxy for sensitivity during calibration.
    let errp_sensitivity = if trial_count > 0 {
        quality.error_trials as f32 / trial_count as f32
    } else {
        0.0
    };

    let errp_specificity = errp_accuracy;
    let tracking_score = (1.0 / (1.0 + quality.avg_tracking_error.max(0.0))).clamp(0.0, 1.0);
    let errp_auc = (0.5 * errp_accuracy + 0.5 * tracking_score).clamp(0.0, 1.0);

    CalibrationQuality {
        errp_accuracy,
        errp_sensitivity,
        errp_specificity,
        errp_auc,
        signal_quality_score: tracking_score,
        trial_count,
        error_trial_count: quality.error_trials,
    }
}
