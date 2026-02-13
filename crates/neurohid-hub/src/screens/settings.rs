//! # Settings Screen
//!
//! Editable `SystemConfig` sections. Each section is a collapsing header
//! with the relevant config fields as egui widgets.

use eframe::egui;

use crate::service_manager::ServiceManager;
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
        service_manager: &ServiceManager,
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

        let mut signal_changed_this_frame = false;

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
                            if ui.text_edit_singleline(&mut lsl_cfg.predicate).changed() {
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

                    if matches!(cfg.backend, neurohid_types::config::DeviceBackend::Serial) {
                        ui.horizontal(|ui| {
                            ui.label("Serial port:");
                            let serial_cfg = cfg.serial.get_or_insert_with(Default::default);
                            let mut port = serial_cfg.port.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut port).changed() {
                                serial_cfg.port = if port.trim().is_empty() {
                                    None
                                } else {
                                    Some(port)
                                };
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Baud:");
                            let serial_cfg = cfg.serial.get_or_insert_with(Default::default);
                            if ui
                                .add(
                                    egui::DragValue::new(&mut serial_cfg.baud_rate)
                                        .clamp_range(1_200..=3_000_000),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Framing:");
                            let serial_cfg = cfg.serial.get_or_insert_with(Default::default);
                            let current = serial_cfg.framing.clone();
                            egui::ComboBox::from_id_source("serial_framing")
                                .selected_text(match serial_cfg.framing {
                                    neurohid_types::config::SerialFraming::CsvLine => "CSV line",
                                    neurohid_types::config::SerialFraming::BinaryI16Le => {
                                        "Binary i16 LE"
                                    }
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut serial_cfg.framing,
                                        neurohid_types::config::SerialFraming::CsvLine,
                                        "CSV line",
                                    );
                                    ui.selectable_value(
                                        &mut serial_cfg.framing,
                                        neurohid_types::config::SerialFraming::BinaryI16Le,
                                        "Binary i16 LE",
                                    );
                                });
                            if serial_cfg.framing != current {
                                changed = true;
                            }
                        });
                    }

                    if matches!(
                        cfg.backend,
                        neurohid_types::config::DeviceBackend::BrainFlow
                    ) {
                        ui.horizontal(|ui| {
                            ui.label("Board id:");
                            let bf_cfg = cfg.brainflow.get_or_insert_with(Default::default);
                            if ui.add(egui::DragValue::new(&mut bf_cfg.board_id)).changed() {
                                changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Serial port:");
                            let bf_cfg = cfg.brainflow.get_or_insert_with(Default::default);
                            let mut port = bf_cfg.serial_port.clone().unwrap_or_default();
                            if ui.text_edit_singleline(&mut port).changed() {
                                bf_cfg.serial_port = if port.trim().is_empty() {
                                    None
                                } else {
                                    Some(port)
                                };
                                changed = true;
                            }
                        });
                    }

                    ui.horizontal(|ui| {
                        ui.label("Preferred device type:");
                        let mut dtype = cfg.preferred_device_type.clone().unwrap_or_default();
                        if ui.text_edit_singleline(&mut dtype).changed() {
                            cfg.preferred_device_type =
                                if dtype.is_empty() { None } else { Some(dtype) };
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Auto-reconnect:");
                        if ui
                            .checkbox(&mut cfg.connection.auto_reconnect, "")
                            .changed()
                        {
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
                            .add(
                                egui::DragValue::new(&mut cfg.notch_filter_hz)
                                    .clamp_range(45.0..=65.0),
                            )
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
                signal_changed_this_frame = true;
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
                            .add(egui::Slider::new(&mut cfg.mouse_sensitivity, 0.1..=5.0).text("x"))
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
                            .add(egui::Slider::new(
                                &mut cfg.min_confidence_threshold,
                                0.0..=1.0,
                            ))
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
                        ui.label("Runtime mode:");
                        let current_mode = cfg.runtime_mode.clone();
                        egui::ComboBox::from_id_source("service_runtime_mode")
                            .selected_text(format!("{}", cfg.runtime_mode))
                            .show_ui(ui, |ui| {
                                for variant in neurohid_types::config::ServiceRuntimeMode::ALL {
                                    ui.selectable_value(
                                        &mut cfg.runtime_mode,
                                        variant.clone(),
                                        format!("{}", variant),
                                    );
                                }
                            });
                        if cfg.runtime_mode != current_mode {
                            changed = true;
                        }
                    });

                    if cfg.runtime_mode == neurohid_types::config::ServiceRuntimeMode::External {
                        ui.horizontal(|ui| {
                            ui.label("Control host:");
                            if ui.text_edit_singleline(&mut cfg.control_host).changed() {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Control port:");
                            if ui
                                .add(
                                    egui::DragValue::new(&mut cfg.control_port)
                                        .clamp_range(1..=65_535),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        });
                        ui.label(
                            egui::RichText::new(
                                "External mode expects a running `neurohid-service --control-port` endpoint.",
                            )
                            .small()
                            .weak(),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new(
                                "Embedded mode runs the runtime inside the hub process.",
                            )
                            .small()
                            .weak(),
                        );
                    }

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

                    ui.separator();
                    ui.label(egui::RichText::new("Latency alert policy").small().strong());

                    ui.horizontal(|ui| {
                        ui.label("Latency alerts:");
                        if ui.checkbox(&mut cfg.latency_alert.enabled, "").changed() {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Decode p95 threshold (us):");
                        if ui
                            .add(
                                egui::DragValue::new(
                                    &mut cfg.latency_alert.decode_p95_threshold_us,
                                )
                                .clamp_range(1_000..=5_000_000)
                                .speed(100.0),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Action p95 threshold (us):");
                        if ui
                            .add(
                                egui::DragValue::new(
                                    &mut cfg.latency_alert.action_p95_threshold_us,
                                )
                                .clamp_range(1_000..=5_000_000)
                                .speed(100.0),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Sustained duration (s):");
                        if ui
                            .add(
                                egui::DragValue::new(
                                    &mut cfg.latency_alert.sustained_duration_secs,
                                )
                                .clamp_range(1..=3_600),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Alert cooldown (s):");
                        if ui
                            .add(
                                egui::DragValue::new(
                                    &mut cfg.latency_alert.notification_cooldown_secs,
                                )
                                .clamp_range(5..=86_400),
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

            // Outlet settings
            let changed = egui::CollapsingHeader::new("Outlets")
                .default_open(false)
                .show(ui, |ui| {
                    let mut changed = false;
                    let cfg = &mut state.config.outlet;

                    ui.horizontal(|ui| {
                        ui.label("Enable outlets:");
                        if ui.checkbox(&mut cfg.enabled, "").changed() {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Publish samples:");
                        if ui.checkbox(&mut cfg.publish_samples, "").changed() {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Publish features:");
                        if ui.checkbox(&mut cfg.publish_features, "").changed() {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Publish actions:");
                        if ui.checkbox(&mut cfg.publish_actions, "").changed() {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Publish markers:");
                        if ui.checkbox(&mut cfg.publish_markers, "").changed() {
                            changed = true;
                        }
                    });

                    if cfg.targets.is_empty() {
                        cfg.targets
                            .push(neurohid_types::config::OutletTarget::default());
                        changed = true;
                    }

                    if let Some(primary) = cfg.targets.first_mut() {
                        ui.separator();
                        ui.label(egui::RichText::new("Primary target").small().strong());

                        ui.horizontal(|ui| {
                            ui.label("Name:");
                            if ui.text_edit_singleline(&mut primary.name).changed() {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Address:");
                            if ui.text_edit_singleline(&mut primary.address).changed() {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Transport:");
                            let old = primary.transport.clone();
                            egui::ComboBox::from_id_source("outlet_transport_primary")
                                .selected_text(match primary.transport {
                                    neurohid_types::config::OutletTransport::TcpJson => "TCP JSON",
                                    neurohid_types::config::OutletTransport::Lsl => "LSL",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut primary.transport,
                                        neurohid_types::config::OutletTransport::TcpJson,
                                        "TCP JSON",
                                    );
                                    ui.selectable_value(
                                        &mut primary.transport,
                                        neurohid_types::config::OutletTransport::Lsl,
                                        "LSL",
                                    );
                                });
                            if primary.transport != old {
                                changed = true;
                            }
                        });
                    }

                    changed
                });
            if changed.body_returned == Some(true) {
                self.unsaved_changes = true;
            }

            // UI settings
            let changed = egui::CollapsingHeader::new("UI")
                .default_open(false)
                .show(ui, |ui| {
                    let mut changed = false;
                    let cfg = &mut state.config.ui;

                    ui.horizontal(|ui| {
                        ui.label("Font scale:");
                        if ui
                            .add(egui::Slider::new(&mut cfg.font_scale, 0.75..=2.0))
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Mode:");
                        let current = cfg.mode.clone();
                        egui::ComboBox::from_id_source("ui_mode")
                            .selected_text(match cfg.mode {
                                neurohid_types::config::UiMode::Standard => "Standard",
                                neurohid_types::config::UiMode::Advanced => "Advanced",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut cfg.mode,
                                    neurohid_types::config::UiMode::Standard,
                                    "Standard",
                                );
                                ui.selectable_value(
                                    &mut cfg.mode,
                                    neurohid_types::config::UiMode::Advanced,
                                    "Advanced",
                                );
                            });
                        if cfg.mode != current {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Theme:");
                        let current = cfg.theme_mode.clone();
                        egui::ComboBox::from_id_source("ui_theme_mode")
                            .selected_text(match cfg.theme_mode {
                                neurohid_types::config::ThemeMode::System => "System",
                                neurohid_types::config::ThemeMode::Light => "Light",
                                neurohid_types::config::ThemeMode::Dark => "Dark",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut cfg.theme_mode,
                                    neurohid_types::config::ThemeMode::System,
                                    "System",
                                );
                                ui.selectable_value(
                                    &mut cfg.theme_mode,
                                    neurohid_types::config::ThemeMode::Light,
                                    "Light",
                                );
                                ui.selectable_value(
                                    &mut cfg.theme_mode,
                                    neurohid_types::config::ThemeMode::Dark,
                                    "Dark",
                                );
                            });
                        if cfg.theme_mode != current {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Pane resizing:");
                        if ui.checkbox(&mut cfg.pane_resize_enabled, "").changed() {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Tray mode:");
                        if ui.checkbox(&mut cfg.tray_mode_enabled, "").changed() {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Auto bootstrap IDE:");
                        if ui.checkbox(&mut cfg.jupyter_auto_bootstrap, "").changed() {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("IDE bootstrap cmd:");
                        if ui
                            .text_edit_singleline(&mut cfg.jupyter_bootstrap_command)
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Jupyter cmd:");
                        if ui.text_edit_singleline(&mut cfg.jupyter_command).changed() {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Jupyter URL:");
                        if ui.text_edit_singleline(&mut cfg.jupyter_url).changed() {
                            changed = true;
                        }
                    });

                    ui.label(
                        egui::RichText::new(
                            "Advanced mode uses managed Jupyter IDE settings above. Legacy lab-kernel config remains only for backward compatibility.",
                        )
                        .small()
                        .weak(),
                    );

                    changed
                });
            if changed.body_returned == Some(true) {
                self.unsaved_changes = true;
            }

            // Recalibration settings
            let changed = egui::CollapsingHeader::new("Recalibration")
                .default_open(false)
                .show(ui, |ui| {
                    let mut changed = false;
                    let cfg = &mut state.config.recalibration;

                    ui.horizontal(|ui| {
                        ui.label("Signal quality threshold:");
                        if ui
                            .add(
                                egui::Slider::new(
                                    &mut cfg.rolling_signal_quality_threshold,
                                    0.0..=1.0,
                                )
                                .show_value(true),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Error rate threshold:");
                        if ui
                            .add(
                                egui::Slider::new(&mut cfg.rolling_error_rate_threshold, 0.0..=1.0)
                                    .show_value(true),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Sustained duration (s):");
                        if ui
                            .add(
                                egui::DragValue::new(&mut cfg.sustained_duration_secs)
                                    .clamp_range(5..=3600),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Prompt cooldown (s):");
                        if ui
                            .add(
                                egui::DragValue::new(&mut cfg.notification_cooldown_secs)
                                    .clamp_range(10..=86_400),
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

        if signal_changed_this_frame {
            service_manager.update_signal_config(state.config.signal.clone());
        }
    }
}
