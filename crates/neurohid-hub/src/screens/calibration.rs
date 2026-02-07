//! # Calibration Screen
//!
//! Wraps `neurohid_calibration::CalibrationPanel` and manages the
//! calibration lifecycle within the hub. Enters/exits calibration mode
//! on the service so that HID emission pauses during calibration.

use eframe::egui;

use neurohid_calibration::panel::{CalibrationPanel, CalibrationPanelResult};

use crate::state::HubState;
use crate::service_manager::ServiceManager;

pub struct CalibrationScreen {
    panel: Option<CalibrationPanel>,
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
            CalibrationPanelResult::Completed(_quality) => {
                service_manager.exit_calibration_mode();
                self.panel = None;
                state.refresh_profiles(runtime);
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

        ui.heading("Calibration");
        ui.add_space(8.0);
        ui.label("Calibrate your brain-computer interface by playing interactive games.");
        ui.label("This process trains the ErrP detector and initial decoder model.");
        ui.add_space(16.0);

        if !snap.running {
            ui.group(|ui| {
                ui.colored_label(egui::Color32::YELLOW, "Service is not running");
                ui.label("Start the service from the Dashboard before calibrating.");
                ui.label("The service provides the device connection needed for calibration.");
            });
            return;
        }

        if !snap.device_connected {
            ui.group(|ui| {
                ui.colored_label(egui::Color32::YELLOW, "No device connected");
                ui.label("Wait for the device to connect, then start calibration.");
            });
            return;
        }

        // Show current calibration status
        if let Some(profile_id) = &state.active_profile_id {
            let profile = state.profiles.iter().find(|p| &p.id == profile_id);
            if let Some(profile) = profile {
                ui.group(|ui| {
                    ui.label(format!("Active profile: {}", profile.name));
                    let cal_status = match &profile.calibration_state {
                        neurohid_types::profile::CalibrationState::NotCalibrated => {
                            "Not calibrated"
                        }
                        neurohid_types::profile::CalibrationState::InProgress { .. } => {
                            "In progress"
                        }
                        neurohid_types::profile::CalibrationState::CompletedGood { .. } => {
                            "Good"
                        }
                        neurohid_types::profile::CalibrationState::CompletedAcceptable { .. } => {
                            "Acceptable"
                        }
                        neurohid_types::profile::CalibrationState::CompletedPoor { .. } => {
                            "Poor"
                        }
                        neurohid_types::profile::CalibrationState::NeedsRecalibration { .. } => {
                            "Needs recalibration"
                        }
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
}
