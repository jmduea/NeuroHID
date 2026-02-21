//! # Training Screen
//!
//! Decoder training configuration and progress. Placeholder for Phase 5
//! primary workflow (device → calibration → train → run).

use eframe::egui;

use crate::service_manager::ServiceManager;
use crate::state::HubState;
use crate::theme;

pub struct TrainingScreen;

impl Default for TrainingScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl TrainingScreen {
    pub fn new() -> Self {
        Self
    }

    /// Renders the training screen (stub: config and progress placeholder).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &HubState,
        _service_manager: &mut ServiceManager,
    ) {
        theme::page_header(
            ui,
            "Training",
            "Configure and run decoder training; view progress and metrics",
        );

        theme::card_frame(ui).show(ui, |ui| {
            theme::status_chip(
                ui,
                "Training — config and progress (stub)",
                theme::Intent::Info,
            );
            if !state.service_snapshot.running {
                theme::status_chip(
                    ui,
                    "Start the service from Dashboard to use training",
                    theme::Intent::Muted,
                );
            }
        });
    }
}
