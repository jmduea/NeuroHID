//! # Settings Screen
//!
//! Editable `SystemConfig` sections. Each section is a collapsing header
//! with the relevant config fields as egui widgets.

use eframe::egui;

use crate::service_manager::ServiceManager;
use crate::state::HubState;
use crate::theme;

pub struct SettingsScreen {
    unsaved_changes: bool,
}

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

        // Save / Reset buttons
        theme::card_frame(ui).show(ui, |ui| {
            ui.horizontal(|ui| {
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

            let reset_clicked =
                theme::action_button(ui, "Reset to Defaults", true, theme::ButtonTone::Secondary);
            if reset_clicked {
                state.config = neurohid_types::config::SystemConfig::default();
                self.unsaved_changes = true;
            }
            if self.unsaved_changes {
                ui.separator();
                ui.colored_label(egui::Color32::from_rgb(248, 205, 95), "Unsaved changes");
            }
        });
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
                        let options = ["Auto", "LSL", "Mock", "Serial", "BrainFlow"];
                        let mut selected = match cfg.backend {
                            neurohid_types::config::DeviceBackend::Auto => 0,
                            neurohid_types::config::DeviceBackend::Lsl => 1,
                            neurohid_types::config::DeviceBackend::Mock => 2,
                            neurohid_types::config::DeviceBackend::Serial => 3,
                            neurohid_types::config::DeviceBackend::BrainFlow => 4,
                        };
                        if theme::select_index(ui, "settings_device_backend", &mut selected, &options, 180.0) {
                            cfg.backend = match selected {
                                0 => neurohid_types::config::DeviceBackend::Auto,
                                1 => neurohid_types::config::DeviceBackend::Lsl,
                                2 => neurohid_types::config::DeviceBackend::Mock,
                                3 => neurohid_types::config::DeviceBackend::Serial,
                                _ => neurohid_types::config::DeviceBackend::BrainFlow,
                            };
                        }
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
                            if theme::text_input(
                                ui,
                                "settings_lsl_predicate",
                                &mut lsl_cfg.predicate,
                                "type='EEG'",
                                260.0,
                            ) {
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
                            if theme::text_input(
                                ui,
                                "settings_serial_port",
                                &mut port,
                                "COM3 / /dev/ttyUSB0",
                                220.0,
                            ) {
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
                            if theme::drag_value(
                                ui,
                                &mut serial_cfg.baud_rate,
                                1_200..=3_000_000,
                                1.0,
                                None,
                            ) {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Framing:");
                            let serial_cfg = cfg.serial.get_or_insert_with(Default::default);
                            let current = serial_cfg.framing.clone();
                            let options = ["CSV line", "Binary i16 LE"];
                            let mut selected = match serial_cfg.framing {
                                neurohid_types::config::SerialFraming::CsvLine => 0,
                                neurohid_types::config::SerialFraming::BinaryI16Le => 1,
                            };
                            if theme::select_index(ui, "settings_serial_framing", &mut selected, &options, 180.0) {
                                serial_cfg.framing = if selected == 0 {
                                    neurohid_types::config::SerialFraming::CsvLine
                                } else {
                                    neurohid_types::config::SerialFraming::BinaryI16Le
                                };
                            }
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
                            if theme::drag_value(
                                ui,
                                &mut bf_cfg.board_id,
                                i32::MIN..=i32::MAX,
                                1.0,
                                None,
                            ) {
                                changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Serial port:");
                            let bf_cfg = cfg.brainflow.get_or_insert_with(Default::default);
                            let mut port = bf_cfg.serial_port.clone().unwrap_or_default();
                            if theme::text_input(
                                ui,
                                "settings_brainflow_serial_port",
                                &mut port,
                                "COM3 / /dev/ttyUSB0",
                                220.0,
                            ) {
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
                        if theme::text_input(
                            ui,
                            "settings_preferred_device_type",
                            &mut dtype,
                            "EEG",
                            200.0,
                        ) {
                            cfg.preferred_device_type =
                                if dtype.is_empty() { None } else { Some(dtype) };
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Auto-reconnect:");
                        if theme::toggle_switch(
                            ui,
                            "settings_auto_reconnect",
                            &mut cfg.connection.auto_reconnect,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Timeout (ms):");
                        let mut val = cfg.connection.connection_timeout_ms as f32;
                        if theme::drag_value(ui, &mut val, 1000.0..=30000.0, 1.0, None) {
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
                        if theme::toggle_switch(
                            ui,
                            "settings_notch_filter_enabled",
                            &mut cfg.notch_filter_enabled,
                        ) {
                            changed = true;
                        }
                        ui.label("Hz:");
                        if theme::drag_value(ui, &mut cfg.notch_filter_hz, 45.0..=65.0, 1.0, None)
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Bandpass filter:");
                        if theme::toggle_switch(
                            ui,
                            "settings_bandpass_filter_enabled",
                            &mut cfg.bandpass_filter_enabled,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Low Hz:");
                        if theme::drag_value(
                            ui,
                            &mut cfg.bandpass_low_hz,
                            0.1..=10.0,
                            0.1,
                            None,
                        ) {
                            changed = true;
                        }
                        ui.label("High Hz:");
                        if theme::drag_value(ui, &mut cfg.bandpass_high_hz, 10.0..=100.0, 1.0, None)
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Window (ms):");
                        if theme::drag_value(ui, &mut cfg.feature_window_ms, 100..=2000, 1.0, None)
                        {
                            changed = true;
                        }
                        ui.label("Step (ms):");
                        if theme::drag_value(ui, &mut cfg.feature_step_ms, 10..=500, 1.0, None) {
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
                        if theme::slider_f32(
                            ui,
                            "settings_mouse_sensitivity",
                            &mut cfg.mouse_sensitivity,
                            0.1,
                            5.0,
                            Some("x"),
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Smoothing:");
                        if theme::toggle_switch(
                            ui,
                            "settings_mouse_smoothing_enabled",
                            &mut cfg.mouse_smoothing_enabled,
                        ) {
                            changed = true;
                        }
                        if cfg.mouse_smoothing_enabled
                            && theme::slider_f32(
                                ui,
                                "settings_mouse_smoothing_factor",
                                &mut cfg.mouse_smoothing_factor,
                                0.0,
                                0.9,
                                Some("factor"),
                            )
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Confidence threshold:");
                        if theme::slider_f32(
                            ui,
                            "settings_min_confidence_threshold",
                            &mut cfg.min_confidence_threshold,
                            0.0,
                            1.0,
                            None,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Debounce (ms):");
                        if theme::drag_value(ui, &mut cfg.action_debounce_ms, 0..=1000, 1.0, None)
                        {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Enabled:");
                        if theme::toggle_switch(ui, "settings_action_enabled", &mut cfg.enabled) {
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
                        if theme::drag_value(
                            ui,
                            &mut cfg.learning_rate,
                            1e-5..=1e-2,
                            1e-5,
                            None,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Gamma:");
                        if theme::drag_value(ui, &mut cfg.gamma, 0.9..=0.999, 0.001, None) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Entropy coef:");
                        if theme::drag_value(ui, &mut cfg.entropy_coef, 0.0..=0.1, 0.001, None) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Online learning:");
                        if theme::toggle_switch(
                            ui,
                            "settings_online_learning_enabled",
                            &mut cfg.online_learning_enabled,
                        ) {
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
                        let options = ["Embedded", "External"];
                        let mut selected = match cfg.runtime_mode {
                            neurohid_types::config::ServiceRuntimeMode::Embedded => 0,
                            neurohid_types::config::ServiceRuntimeMode::External => 1,
                        };
                        if theme::select_index(ui, "settings_service_runtime_mode", &mut selected, &options, 180.0) {
                            cfg.runtime_mode = if selected == 0 {
                                neurohid_types::config::ServiceRuntimeMode::Embedded
                            } else {
                                neurohid_types::config::ServiceRuntimeMode::External
                            };
                        }
                        if cfg.runtime_mode != current_mode {
                            changed = true;
                        }
                    });

                    if cfg.runtime_mode == neurohid_types::config::ServiceRuntimeMode::External {
                        ui.horizontal(|ui| {
                            ui.label("Control host:");
                            if theme::text_input(
                                ui,
                                "settings_control_host",
                                &mut cfg.control_host,
                                "127.0.0.1",
                                220.0,
                            ) {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Control port:");
                            if theme::drag_value(
                                ui,
                                &mut cfg.control_port,
                                1..=65_535,
                                1.0,
                                None,
                            ) {
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
                        let _ = theme::select_index(
                            ui,
                            "settings_log_level",
                            &mut selected,
                            &levels,
                            140.0,
                        );
                        if selected != current {
                            cfg.log_level = levels[selected].to_string();
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Notifications:");
                        if theme::toggle_switch(
                            ui,
                            "settings_notifications_enabled",
                            &mut cfg.notifications_enabled,
                        ) {
                            changed = true;
                        }
                    });

                    ui.separator();
                    ui.label(egui::RichText::new("Latency alert policy").small().strong());

                    ui.horizontal(|ui| {
                        ui.label("Latency alerts:");
                        if theme::toggle_switch(
                            ui,
                            "settings_latency_alert_enabled",
                            &mut cfg.latency_alert.enabled,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Decode p95 threshold (us):");
                        if theme::drag_value(
                            ui,
                            &mut cfg.latency_alert.decode_p95_threshold_us,
                            1_000..=5_000_000,
                            100.0,
                            None,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Action p95 threshold (us):");
                        if theme::drag_value(
                            ui,
                            &mut cfg.latency_alert.action_p95_threshold_us,
                            1_000..=5_000_000,
                            100.0,
                            None,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Sustained duration (s):");
                        if theme::drag_value(
                            ui,
                            &mut cfg.latency_alert.sustained_duration_secs,
                            1..=3_600,
                            1.0,
                            None,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Alert cooldown (s):");
                        if theme::drag_value(
                            ui,
                            &mut cfg.latency_alert.notification_cooldown_secs,
                            5..=86_400,
                            1.0,
                            None,
                        ) {
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
                        if theme::toggle_switch(ui, "settings_outlets_enabled", &mut cfg.enabled)
                        {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Publish samples:");
                        if theme::toggle_switch(
                            ui,
                            "settings_publish_samples",
                            &mut cfg.publish_samples,
                        ) {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Publish features:");
                        if theme::toggle_switch(
                            ui,
                            "settings_publish_features",
                            &mut cfg.publish_features,
                        ) {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Publish actions:");
                        if theme::toggle_switch(
                            ui,
                            "settings_publish_actions",
                            &mut cfg.publish_actions,
                        ) {
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Publish markers:");
                        if theme::toggle_switch(
                            ui,
                            "settings_publish_markers",
                            &mut cfg.publish_markers,
                        ) {
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
                            if theme::text_input(
                                ui,
                                "settings_outlet_name",
                                &mut primary.name,
                                "primary",
                                220.0,
                            ) {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Address:");
                            if theme::text_input(
                                ui,
                                "settings_outlet_address",
                                &mut primary.address,
                                "127.0.0.1:9000",
                                220.0,
                            ) {
                                changed = true;
                            }
                        });

                        ui.horizontal(|ui| {
                            ui.label("Transport:");
                            let old = primary.transport.clone();
                            let options = ["TCP JSON", "LSL"];
                            let mut selected = match primary.transport {
                                neurohid_types::config::OutletTransport::TcpJson => 0,
                                neurohid_types::config::OutletTransport::Lsl => 1,
                            };
                            if theme::select_index(
                                ui,
                                "settings_outlet_transport_primary",
                                &mut selected,
                                &options,
                                180.0,
                            ) {
                                primary.transport = if selected == 0 {
                                    neurohid_types::config::OutletTransport::TcpJson
                                } else {
                                    neurohid_types::config::OutletTransport::Lsl
                                };
                            }
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
                        if theme::slider_f32(
                            ui,
                            "settings_ui_font_scale",
                            &mut cfg.font_scale,
                            0.75,
                            2.0,
                            None,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Mode:");
                        let current = cfg.mode.clone();
                        let options = ["Standard", "Advanced"];
                        let mut selected = if cfg.mode == neurohid_types::config::UiMode::Standard {
                            0
                        } else {
                            1
                        };
                        if theme::select_index(ui, "settings_ui_mode", &mut selected, &options, 160.0) {
                            cfg.mode = if selected == 0 {
                                neurohid_types::config::UiMode::Standard
                            } else {
                                neurohid_types::config::UiMode::Advanced
                            };
                        }
                        if cfg.mode != current {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Theme:");
                        let current = cfg.theme_mode.clone();
                        let options = ["System", "Light", "Dark"];
                        let mut selected = match cfg.theme_mode {
                            neurohid_types::config::ThemeMode::System => 0,
                            neurohid_types::config::ThemeMode::Light => 1,
                            neurohid_types::config::ThemeMode::Dark => 2,
                        };
                        if theme::select_index(ui, "settings_ui_theme_mode", &mut selected, &options, 160.0) {
                            cfg.theme_mode = match selected {
                                0 => neurohid_types::config::ThemeMode::System,
                                1 => neurohid_types::config::ThemeMode::Light,
                                _ => neurohid_types::config::ThemeMode::Dark,
                            };
                        }
                        if cfg.theme_mode != current {
                            changed = true;
                        }
                    });

                    ui.label(
                        egui::RichText::new("Visualization docking backend: egui_dock (standard)")
                            .small()
                            .weak(),
                    );

                    ui.horizontal(|ui| {
                        ui.label("Pane resizing:");
                        if theme::toggle_switch(
                            ui,
                            "settings_pane_resize_enabled",
                            &mut cfg.pane_resize_enabled,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Tray mode:");
                        if theme::toggle_switch(
                            ui,
                            "settings_tray_mode_enabled",
                            &mut cfg.tray_mode_enabled,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Auto bootstrap IDE:");
                        if theme::toggle_switch(
                            ui,
                            "settings_jupyter_auto_bootstrap",
                            &mut cfg.jupyter_auto_bootstrap,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("IDE bootstrap cmd:");
                        if theme::text_input(
                            ui,
                            "settings_jupyter_bootstrap_command",
                            &mut cfg.jupyter_bootstrap_command,
                            "uv sync",
                            260.0,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Jupyter cmd:");
                        if theme::text_input(
                            ui,
                            "settings_jupyter_command",
                            &mut cfg.jupyter_command,
                            "uv run jupyter lab",
                            260.0,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Jupyter URL:");
                        if theme::text_input(
                            ui,
                            "settings_jupyter_url",
                            &mut cfg.jupyter_url,
                            "http://127.0.0.1:8888/lab",
                            260.0,
                        ) {
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
                        if theme::slider_f32(
                            ui,
                            "settings_rolling_signal_quality_threshold",
                            &mut cfg.rolling_signal_quality_threshold,
                            0.0,
                            1.0,
                            None,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Error rate threshold:");
                        if theme::slider_f32(
                            ui,
                            "settings_rolling_error_rate_threshold",
                            &mut cfg.rolling_error_rate_threshold,
                            0.0,
                            1.0,
                            None,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Sustained duration (s):");
                        if theme::drag_value(
                            ui,
                            &mut cfg.sustained_duration_secs,
                            5..=3600,
                            1.0,
                            None,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Prompt cooldown (s):");
                        if theme::drag_value(
                            ui,
                            &mut cfg.notification_cooldown_secs,
                            10..=86_400,
                            1.0,
                            None,
                        ) {
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
                        if theme::toggle_switch(
                            ui,
                            "settings_storage_encryption_enabled",
                            &mut cfg.encryption_enabled,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Session logging:");
                        if theme::toggle_switch(
                            ui,
                            "settings_session_logging_enabled",
                            &mut cfg.session_logging_enabled,
                        ) {
                            changed = true;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Retention (days):");
                        if theme::drag_value(
                            ui,
                            &mut cfg.session_log_retention_days,
                            1..=365,
                            1.0,
                            None,
                        ) {
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

