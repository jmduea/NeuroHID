//! # Settings Screen
//!
//! Editable `SystemConfig` sections. Each section is a collapsing header
//! with the relevant config fields as egui widgets.

use eframe::egui;

use neurohid_core::extension_registry::{default_extension_paths, ExtensionRegistry};

use crate::service_manager::ServiceManager;
use crate::state::HubState;
use crate::theme;

pub struct SettingsScreen {
    unsaved_changes: bool,
}

mod device;
mod pipeline;
mod system;
mod ui_prefs;

impl Default for SettingsScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsScreen {
    pub fn new() -> Self {
        Self {
            unsaved_changes: false,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &mut HubState,
        service_manager: &ServiceManager,
        runtime: &tokio::runtime::Runtime,
    ) {
        theme::page_header(
            ui,
            "Settings",
            "Configure runtime behavior, signal pipeline, devices, and interfaces",
        );

        theme::card_frame(ui).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                theme::status_chip(
                    ui,
                    if self.unsaved_changes {
                        "Unsaved changes"
                    } else {
                        "All changes saved"
                    },
                    if self.unsaved_changes {
                        theme::Intent::Warning
                    } else {
                        theme::Intent::Success
                    },
                );

                let runtime_mode_label = match state.config.service.runtime_mode {
                    neurohid_types::config::ServiceRuntimeMode::Embedded => "Runtime embedded",
                    neurohid_types::config::ServiceRuntimeMode::External => "Runtime external",
                };
                theme::status_chip(ui, runtime_mode_label, theme::Intent::Info);

                let ui_mode_label =
                    if state.config.ui.mode == neurohid_types::config::UiMode::Advanced {
                        "UI advanced"
                    } else {
                        "UI standard"
                    };
                theme::status_chip(ui, ui_mode_label, theme::Intent::Info);

                let backend_label = match &state.config.device.backend {
                    neurohid_types::config::DeviceBackend::Auto => "Backend auto".to_string(),
                    neurohid_types::config::DeviceBackend::Lsl => "Backend LSL".to_string(),
                    neurohid_types::config::DeviceBackend::Mock => "Backend mock".to_string(),
                    neurohid_types::config::DeviceBackend::Serial => "Backend serial".to_string(),
                    neurohid_types::config::DeviceBackend::BrainFlow => "Backend BrainFlow".to_string(),
                    neurohid_types::config::DeviceBackend::Extension(name) => {
                        format!("Backend extension({})", name)
                    }
                };
                theme::status_chip(ui, &backend_label, theme::Intent::Muted);

                theme::status_chip(
                    ui,
                    if state.config.service.notifications_enabled {
                        "Notifications on"
                    } else {
                        "Notifications off"
                    },
                    if state.config.service.notifications_enabled {
                        theme::Intent::Info
                    } else {
                        theme::Intent::Muted
                    },
                );
            });
            ui.add_space(6.0);
            theme::status_chip(
                ui,
                "Tip: Save commits all modified sections at once",
                theme::Intent::Muted,
            );
        });

        ui.add_space(10.0);

        // Save / Reset buttons
        theme::card_frame(ui).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let save_clicked = theme::action_button(
                    ui,
                    "Save",
                    self.unsaved_changes,
                    theme::ButtonTone::Primary,
                );
                if save_clicked {
                    match runtime.block_on(state.config_store.save(&state.config)) {
                        Ok(()) => {
                            tracing::info!("Configuration saved");
                            self.unsaved_changes = false;
                        }
                        Err(e) => tracing::error!("Failed to save config: {}", e),
                    }
                }

                let reset_clicked = theme::action_button(
                    ui,
                    "Reset to Defaults",
                    true,
                    theme::ButtonTone::Secondary,
                );
                if reset_clicked {
                    state.config = neurohid_types::config::SystemConfig::default();
                    self.unsaved_changes = true;
                }
                if self.unsaved_changes {
                    theme::status_chip(ui, "Unsaved changes", theme::Intent::Warning);
                }
            });
        });

        ui.add_space(16.0);


        egui::ScrollArea::vertical().show(ui, |ui| {
            if device::render(ui, state, runtime) {
                self.unsaved_changes = true;
            }
            if pipeline::render(ui, state, service_manager, runtime) {
                self.unsaved_changes = true;
            }
            if system::render(ui, state, runtime) {
                self.unsaved_changes = true;
            }
            if ui_prefs::render(ui, state, runtime) {
                self.unsaved_changes = true;
            }
        });
    }
}
