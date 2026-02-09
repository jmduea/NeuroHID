//! # Settings Screen
//!
//! Editable `SystemConfig` sections. Each section is a collapsing header
//! with the relevant config fields as egui widgets.

use eframe::egui;

use crate::state::HubState;

pub struct SettingsScreen {
    unsaved_changes: bool,
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
        runtime: &tokio::runtime::Runtime,
    ) {
        ui.heading("Settings");
        ui.add_space(8.0);

        // Save / Reset buttons
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.unsaved_changes, egui::Button::new("Save"))
                .clicked()
            {
                match runtime.block_on(state.config_store.save(&state.config)) {
                    Ok(()) => {
                        tracing::info!("Configuration saved");
                        self.unsaved_changes = false;
                    }
                    Err(e) => tracing::error!("Failed to save config: {}", e),
                }
            }

            if ui.button("Reset to Defaults").clicked() {
                state.config = neurohid_types::config::SystemConfig::default();
                self.unsaved_changes = true;
            }
        });

        ui.add_space(16.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            // Device settings
            let changed = egui::CollapsingHeader::new("Device")
                .default_open(true)
                .show(ui, |ui| {
                    let mut changed = false;
                    let cfg = &mut state.config.device;

                    // Device backend selector
                    ui.horizontal(|ui| {
                        ui.label("Backend:");
                        let current_backend = cfg.backend.clone();
                        egui::ComboBox::from_id_source("device_backend")
                            .selected_text(format!("{}", cfg.backend))
                            .show_ui(ui, |ui| {
                                for variant in neurohid_types::config::DeviceBackend::ALL {
                                    ui.selectable_value(
                                        &mut cfg.backend,
                                        variant.clone(),
                                        format!("{}", variant),
                                    );
                                }
                            });
                        if cfg.backend != current_backend {
                            changed = true;
                        }
                    });

                    // LSL predicate field (visible when backend is Lsl or Auto)
                    if matches!(
                        cfg.backend,
                        neurohid_types::config::DeviceBackend::Lsl
                            | neurohid_types::config::DeviceBackend::Auto
                    ) {
                        ui.horizontal(|ui| {
                            ui.label("LSL predicate:");
                            let lsl_cfg = cfg.lsl.get_or_insert_with(Default::default);
                            if ui
                                .text_edit_singleline(&mut lsl_cfg.predicate)
                                .changed()
                            {
                                changed = true;
                            }
                        });
                        ui.label(
                            egui::RichText::new(
                                "Filter streams by predicate, e.g. \"type='EEG'\" \
                                 or leave empty for all streams.",
                            )
                            .small()
                            .weak(),
                        );
                    }

                    ui.horizontal(|ui| {
                        ui.label("Preferred device type:");
                        let mut dtype = cfg.preferred_device_type.clone().unwrap_or_default();
                        if ui.text_edit_singleline(&mut dtype).changed() {
                            cfg.preferred_device_type = if dtype.is_empty() {
                                None
                            } else {
                                Some(dtype)
                            };
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Auto-reconnect:");
                        if ui.checkbox(&mut cfg.connection.auto_reconnect, "").changed() {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Timeout (ms):");
                        let mut val = cfg.connection.connection_timeout_ms as f32;
                        if ui
                            .add(egui::DragValue::new(&mut val).clamp_range(1000.0..=30000.0))
                            .changed()
                        {
                            cfg.connection.connection_timeout_ms = val as u64;
                            changed = true;
                        }
                    });

                    changed
                });
            if changed.body_returned == Some(true) {
                self.unsaved_changes = true;
            }

            // Signal settings
            let changed = egui::CollapsingHeader::new("Signal Processing")
                .default_open(false)
                .show(ui, |ui| {
                    let mut changed = false;
                    let cfg = &mut state.config.signal;

                    ui.horizontal(|ui| {
                        ui.label("Notch filter:");
                        if ui.checkbox(&mut cfg.notch_filter_enabled, "").changed() {
                            changed = true;
                        }
                        ui.label("Hz:");
                        if ui
                            .add(egui::DragValue::new(&mut cfg.notch_filter_hz).clamp_range(45.0..=65.0))
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Bandpass filter:");
                        if ui.checkbox(&mut cfg.bandpass_filter_enabled, "").changed() {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Low Hz:");
                        if ui
                            .add(
                                egui::DragValue::new(&mut cfg.bandpass_low_hz)
                                    .clamp_range(0.1..=10.0)
                                    .speed(0.1),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                        ui.label("High Hz:");
                        if ui
                            .add(
                                egui::DragValue::new(&mut cfg.bandpass_high_hz)
                                    .clamp_range(10.0..=100.0),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Window (ms):");
                        if ui
                            .add(
                                egui::DragValue::new(&mut cfg.feature_window_ms)
                                    .clamp_range(100..=2000),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                        ui.label("Step (ms):");
                        if ui
                            .add(
                                egui::DragValue::new(&mut cfg.feature_step_ms)
                                    .clamp_range(10..=500),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    changed
                });
            if changed.body_returned == Some(true) {
                self.unsaved_changes = true;
            }

            // Action settings
            let changed = egui::CollapsingHeader::new("Action")
                .default_open(false)
                .show(ui, |ui| {
                    let mut changed = false;
                    let cfg = &mut state.config.action;

                    ui.horizontal(|ui| {
                        ui.label("Sensitivity:");
                        if ui
                            .add(
                                egui::Slider::new(&mut cfg.mouse_sensitivity, 0.1..=5.0)
                                    .text("x"),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Smoothing:");
                        if ui.checkbox(&mut cfg.mouse_smoothing_enabled, "").changed() {
                            changed = true;
                        }
                        if cfg.mouse_smoothing_enabled
                            && ui
                                .add(
                                    egui::Slider::new(&mut cfg.mouse_smoothing_factor, 0.0..=0.9)
                                        .text("factor"),
                                )
                                .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Confidence threshold:");
                        if ui
                            .add(
                                egui::Slider::new(&mut cfg.min_confidence_threshold, 0.0..=1.0),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Debounce (ms):");
                        if ui
                            .add(
                                egui::DragValue::new(&mut cfg.action_debounce_ms)
                                    .clamp_range(0..=1000),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Enabled:");
                        if ui.checkbox(&mut cfg.enabled, "").changed() {
                            changed = true;
                        }
                    });

                    changed
                });
            if changed.body_returned == Some(true) {
                self.unsaved_changes = true;
            }

            // Decoder settings (advanced)
            let changed = egui::CollapsingHeader::new("Decoder (Advanced)")
                .default_open(false)
                .show(ui, |ui| {
                    let mut changed = false;
                    let cfg = &mut state.config.decoder;

                    ui.horizontal(|ui| {
                        ui.label("Learning rate:");
                        if ui
                            .add(
                                egui::DragValue::new(&mut cfg.learning_rate)
                                    .clamp_range(1e-5..=1e-2)
                                    .speed(1e-5),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Gamma:");
                        if ui
                            .add(
                                egui::DragValue::new(&mut cfg.gamma)
                                    .clamp_range(0.9..=0.999)
                                    .speed(0.001),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Entropy coef:");
                        if ui
                            .add(
                                egui::DragValue::new(&mut cfg.entropy_coef)
                                    .clamp_range(0.0..=0.1)
                                    .speed(0.001),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Online learning:");
                        if ui.checkbox(&mut cfg.online_learning_enabled, "").changed() {
                            changed = true;
                        }
                    });

                    changed
                });
            if changed.body_returned == Some(true) {
                self.unsaved_changes = true;
            }

            // Service settings
            let changed = egui::CollapsingHeader::new("Service")
                .default_open(false)
                .show(ui, |ui| {
                    let mut changed = false;
                    let cfg = &mut state.config.service;

                    ui.horizontal(|ui| {
                        ui.label("Log level:");
                        let levels = ["trace", "debug", "info", "warn", "error"];
                        let current = levels.iter().position(|&l| l == cfg.log_level).unwrap_or(2);
                        let mut selected = current;
                        egui::ComboBox::from_label("")
                            .selected_text(levels[selected])
                            .show_ui(ui, |ui| {
                                for (i, &level) in levels.iter().enumerate() {
                                    ui.selectable_value(&mut selected, i, level);
                                }
                            });
                        if selected != current {
                            cfg.log_level = levels[selected].to_string();
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Notifications:");
                        if ui.checkbox(&mut cfg.notifications_enabled, "").changed() {
                            changed = true;
                        }
                    });

                    changed
                });
            if changed.body_returned == Some(true) {
                self.unsaved_changes = true;
            }

            // Storage settings
            let changed = egui::CollapsingHeader::new("Storage")
                .default_open(false)
                .show(ui, |ui| {
                    let mut changed = false;
                    let cfg = &mut state.config.storage;

                    ui.horizontal(|ui| {
                        ui.label("Encryption:");
                        if ui.checkbox(&mut cfg.encryption_enabled, "").changed() {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Session logging:");
                        if ui.checkbox(&mut cfg.session_logging_enabled, "").changed() {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Retention (days):");
                        if ui
                            .add(
                                egui::DragValue::new(&mut cfg.session_log_retention_days)
                                    .clamp_range(1..=365),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    changed
                });
            if changed.body_returned == Some(true) {
                self.unsaved_changes = true;
            }
        });
    }
}
