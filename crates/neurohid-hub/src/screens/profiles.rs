//! # Profiles Screen
//!
//! Profile CRUD: create, delete, set active, and view calibration status.
//! Each profile stores a user's personalized decoder weights and ErrP model.

use eframe::egui;

use neurohid_types::profile::CalibrationState;

use crate::service_manager::ServiceManager;
use crate::state::HubState;
use crate::theme;

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
        theme::page_header(
            ui,
            "Profiles",
            "Each profile stores personalized calibration data and decoder weights.",
        );
        ui.add_space(6.0);

        let profile_count = state.profiles.len();
        let calibrated_count = state
            .profiles
            .iter()
            .filter(|profile| profile.calibration_state.is_ready())
            .count();
        let has_active = state.active_profile_id.is_some();
        theme::card_frame(ui).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                theme::status_chip(
                    ui,
                    &format!("Profiles {}", profile_count),
                    if profile_count > 0 {
                        theme::Intent::Info
                    } else {
                        theme::Intent::Muted
                    },
                );
                theme::status_chip(
                    ui,
                    &format!("Calibrated {}", calibrated_count),
                    if calibrated_count > 0 {
                        theme::Intent::Success
                    } else {
                        theme::Intent::Warning
                    },
                );
                theme::status_chip(
                    ui,
                    if has_active {
                        "Active profile set"
                    } else {
                        "No active profile"
                    },
                    if has_active {
                        theme::Intent::Success
                    } else {
                        theme::Intent::Warning
                    },
                );
            });
        });
        ui.add_space(8.0);

        // Create profile button
        if !self.show_create_dialog
            && theme::action_button(ui, "Create New Profile", true, theme::ButtonTone::Primary)
        {
            self.show_create_dialog = true;
            self.new_profile_name.clear();
        }

        // Create dialog
        if self.show_create_dialog {
            theme::card_frame(ui).show(ui, |ui| {
                ui.heading("New Profile");
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let _ = theme::text_input(
                        ui,
                        "profiles_new_profile_name",
                        &mut self.new_profile_name,
                        "Profile name",
                        240.0,
                    );
                });
                ui.horizontal(|ui| {
                    let name_valid = !self.new_profile_name.trim().is_empty();
                    if theme::action_button(ui, "Create", name_valid, theme::ButtonTone::Primary)
                    {
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
                    if theme::action_button(ui, "Cancel", true, theme::ButtonTone::Ghost) {
                        self.show_create_dialog = false;
                    }
                });
                });
        }

        ui.add_space(16.0);

        // Profile list
        if state.profiles.is_empty() {
            theme::status_chip(ui, "No profiles yet", theme::Intent::Warning);
            theme::status_chip(ui, "Create a profile to get started", theme::Intent::Muted);
            return;
        }

        // Handle delete confirmation
        let mut delete_id = None;
        if let Some(id_str) = &self.delete_confirm {
            let id_str = id_str.clone();
            theme::card_frame(ui)
                .fill(egui::Color32::from_rgb(40, 20, 24))
                .show(ui, |ui| {
                theme::status_chip(
                    ui,
                    &format!("Delete profile \"{}\"?", id_str),
                    theme::Intent::Danger,
                );
                ui.horizontal(|ui| {
                    if theme::action_button(ui, "Yes, Delete", true, theme::ButtonTone::Ghost) {
                        delete_id = Some(id_str.clone());
                        self.delete_confirm = None;
                    }
                    if theme::action_button(ui, "Cancel", true, theme::ButtonTone::Secondary) {
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

            theme::card_frame(ui).show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Active indicator
                    if is_active {
                        theme::status_chip(ui, "Active", theme::Intent::Success);
                    } else {
                        theme::status_chip(ui, "Inactive", theme::Intent::Muted);
                    }

                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(&profile.name);

                            // Calibration badge
                            let (badge_text, badge_intent) = match &profile.calibration_state {
                                CalibrationState::CompletedGood { .. } => {
                                    ("Good", theme::Intent::Success)
                                }
                                CalibrationState::CompletedAcceptable { .. } => {
                                    ("Acceptable", theme::Intent::Warning)
                                }
                                CalibrationState::CompletedPoor { .. } => {
                                    ("Poor", theme::Intent::Danger)
                                }
                                CalibrationState::InProgress { .. } => {
                                    ("In Progress", theme::Intent::Info)
                                }
                                CalibrationState::NeedsRecalibration { .. } => {
                                    ("Needs Recalibration", theme::Intent::Danger)
                                }
                                CalibrationState::NotCalibrated => {
                                    ("Not Calibrated", theme::Intent::Muted)
                                }
                            };
                            theme::status_chip(ui, badge_text, badge_intent);
                        });

                        ui.label(
                            egui::RichText::new(format!("ID: {}", profile.id))
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    });

                    // Action buttons (right-aligned)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::action_button(ui, "Delete", true, theme::ButtonTone::Ghost) {
                            self.delete_confirm = Some(profile.id.to_string());
                        }

                        if !is_active
                            && theme::action_button(
                                ui,
                                "Set Active",
                                true,
                                theme::ButtonTone::Primary,
                            )
                        {
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
