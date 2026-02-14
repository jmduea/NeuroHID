//! # Profiles Screen
//!
//! Profile CRUD: create, delete, set active, and view calibration status.
//! Each profile stores a user's personalized decoder weights and ErrP model.

use eframe::egui;

use neurohid_types::profile::CalibrationState;

use crate::service_manager::ServiceManager;
use crate::state::HubState;

pub struct ProfilesScreen {
    new_profile_name: String,
    show_create_dialog: bool,
    delete_confirm: Option<String>,
}

impl Default for ProfilesScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfilesScreen {
    pub fn new() -> Self {
        Self {
            new_profile_name: String::new(),
            show_create_dialog: false,
            delete_confirm: None,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut HubState,
        runtime: &tokio::runtime::Runtime,
        service_manager: &ServiceManager,
    ) {
        ui.label(
            egui::RichText::new("Profiles")
                .text_style(egui::TextStyle::Heading)
                .color(egui::Color32::from_rgb(225, 233, 245)),
        );
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("Each profile stores personalized calibration data and decoder weights.")
                .small()
                .color(egui::Color32::from_rgb(128, 145, 167)),
        );
        ui.add_space(16.0);

        // Create profile button
        if !self.show_create_dialog && ui.button("Create New Profile").clicked() {
            self.show_create_dialog = true;
            self.new_profile_name.clear();
        }

        // Create dialog
        if self.show_create_dialog {
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(20, 25, 34))
                .show(ui, |ui| {
                ui.heading("New Profile");
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.new_profile_name);
                });
                ui.horizontal(|ui| {
                    let name_valid = !self.new_profile_name.trim().is_empty();
                    ui.add_enabled_ui(name_valid, |ui| {
                        if ui.button("Create").clicked() {
                            let name = self.new_profile_name.trim().to_string();
                            match runtime.block_on(state.profile_store.create_profile(name)) {
                                Ok(metadata) => {
                                    tracing::info!("Created profile: {}", metadata.id);
                                    if state.active_profile_id.is_none() {
                                        state.active_profile_id = Some(metadata.id.clone());
                                        service_manager.set_active_profile(
                                            Some(metadata.id),
                                            metadata.name,
                                            false,
                                        );
                                    }
                                    state.refresh_profiles(runtime);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to create profile: {}", e);
                                }
                            }
                            self.show_create_dialog = false;
                        }
                    });
                    if ui.button("Cancel").clicked() {
                        self.show_create_dialog = false;
                    }
                });
                });
        }

        ui.add_space(16.0);

        // Profile list
        if state.profiles.is_empty() {
            ui.label("No profiles yet. Create one to get started.");
            return;
        }

        // Handle delete confirmation
        let mut delete_id = None;
        if let Some(id_str) = &self.delete_confirm {
            let id_str = id_str.clone();
            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(34, 20, 24))
                .show(ui, |ui| {
                ui.colored_label(
                    egui::Color32::RED,
                    format!("Delete profile \"{}\"?", id_str),
                );
                ui.horizontal(|ui| {
                    if ui.button("Yes, Delete").clicked() {
                        delete_id = Some(id_str.clone());
                        self.delete_confirm = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.delete_confirm = None;
                    }
                });
                });
            ui.add_space(8.0);
        }

        // Execute delete if confirmed
        if let Some(id_str) = delete_id {
            let profile_id = neurohid_types::profile::ProfileId::new(&id_str);
            match runtime.block_on(state.profile_store.delete_profile(&profile_id)) {
                Ok(()) => {
                    tracing::info!("Deleted profile: {}", id_str);
                    if state.active_profile_id.as_ref().map(|p| p.to_string()) == Some(id_str) {
                        state.active_profile_id = None;
                        service_manager.set_active_profile(None, "none".to_string(), false);
                    }
                    state.refresh_profiles(runtime);
                }
                Err(e) => tracing::error!("Failed to delete profile: {}", e),
            }
        }

        // Render profile cards
        for profile in &state.profiles {
            let is_active = state.active_profile_id.as_ref() == Some(&profile.id);

            egui::Frame::group(ui.style())
                .fill(egui::Color32::from_rgb(20, 25, 34))
                .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Active indicator
                    if is_active {
                        ui.colored_label(egui::Color32::GREEN, "●");
                    } else {
                        ui.colored_label(egui::Color32::GRAY, "○");
                    }

                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(&profile.name);

                            // Calibration badge
                            let (badge_color, badge_text) = match &profile.calibration_state {
                                CalibrationState::CompletedGood { .. } => {
                                    (egui::Color32::GREEN, "Good")
                                }
                                CalibrationState::CompletedAcceptable { .. } => {
                                    (egui::Color32::YELLOW, "Acceptable")
                                }
                                CalibrationState::CompletedPoor { .. } => {
                                    (egui::Color32::RED, "Poor")
                                }
                                CalibrationState::InProgress { .. } => {
                                    (egui::Color32::GRAY, "In Progress")
                                }
                                CalibrationState::NeedsRecalibration { .. } => {
                                    (egui::Color32::RED, "Needs Recalibration")
                                }
                                CalibrationState::NotCalibrated => {
                                    (egui::Color32::GRAY, "Not Calibrated")
                                }
                            };

                            ui.colored_label(badge_color, format!("[{}]", badge_text));
                        });

                        ui.label(
                            egui::RichText::new(format!("ID: {}", profile.id))
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    });

                    // Action buttons (right-aligned)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Delete").clicked() {
                            self.delete_confirm = Some(profile.id.to_string());
                        }

                        if !is_active && ui.small_button("Set Active").clicked() {
                            state.active_profile_id = Some(profile.id.clone());
                            service_manager.set_active_profile(
                                Some(profile.id.clone()),
                                profile.name.clone(),
                                profile.calibration_state.is_ready(),
                            );
                        }
                    });
                });
                });
        }
    }
}
