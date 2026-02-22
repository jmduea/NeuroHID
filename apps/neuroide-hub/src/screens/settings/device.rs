use eframe::egui;
use neurohid_core::extension_registry::{default_extension_paths, ExtensionRegistry};
use crate::theme;

pub fn render(ui: &mut egui::Ui, state: &mut crate::state::HubState, runtime: &tokio::runtime::Runtime) -> bool {
    let mut unsaved = false;

            ui.label(egui::RichText::new("Device & acquisition").small().weak());
            let last_device_backend = state.config.device.backend.clone();
            // Device settings
            let changed = egui::CollapsingHeader::new("Device")
                .default_open(true)
                .show(ui, |ui| {
                    let mut changed = false;
                    let cfg = &mut state.config.device;

                    // Device backend selector: built-in + discovered device extensions
                    ui.horizontal(|ui| {
                        ui.label("Backend:");
                        let current_backend = cfg.backend.clone();
                        let mut reg =
                            ExtensionRegistry::new(default_extension_paths());
                        let _ = reg.scan();
                        let device_names: Vec<String> =
                            reg.list_devices().into_iter().map(|e| e.name).collect();
                        let builtin =
                            ["Auto", "LSL", "Mock", "Serial", "BrainFlow"];
                        let options: Vec<String> = builtin
                            .iter()
                            .map(|s| (*s).to_string())
                            .chain(device_names.clone())
                            .collect();
                        let options_ref: Vec<&str> =
                            options.iter().map(String::as_str).collect();
                        let mut selected = match &cfg.backend {
                            neurohid_types::config::DeviceBackend::Auto => 0,
                            neurohid_types::config::DeviceBackend::Lsl => 1,
                            neurohid_types::config::DeviceBackend::Mock => 2,
                            neurohid_types::config::DeviceBackend::Serial => 3,
                            neurohid_types::config::DeviceBackend::BrainFlow => 4,
                            neurohid_types::config::DeviceBackend::Extension(name) => options
                                .iter()
                                .position(|s| s == name)
                                .unwrap_or(5.min(options.len().saturating_sub(1))),
                        };
                        if theme::select_index(
                            ui,
                            "settings_device_backend",
                            &mut selected,
                            &options_ref,
                            180.0,
                        ) {
                            cfg.backend = if selected < 5 {
                                match selected {
                                    0 => neurohid_types::config::DeviceBackend::Auto,
                                    1 => neurohid_types::config::DeviceBackend::Lsl,
                                    2 => neurohid_types::config::DeviceBackend::Mock,
                                    3 => neurohid_types::config::DeviceBackend::Serial,
                                    4 => neurohid_types::config::DeviceBackend::BrainFlow,
                                    _ => neurohid_types::config::DeviceBackend::Auto,
                                }
                            } else if let Some(name) = options.get(selected) {
                                neurohid_types::config::DeviceBackend::Extension(name.clone())
                            } else {
                                neurohid_types::config::DeviceBackend::Auto
                            };
                            changed = true;
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
                        theme::status_chip(
                            ui,
                            "Example: type='EEG' (leave empty for all streams)",
                            theme::Intent::Muted,
                        );
                    }

                    if matches!(cfg.backend, neurohid_types::config::DeviceBackend::Serial) {
                        theme::status_chip(
                            ui,
                            "Serial backend: configure port and baud in Settings.",
                            theme::Intent::Warning,
                        );
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
                            if theme::select_index(
                                ui,
                                "settings_serial_framing",
                                &mut selected,
                                &options,
                                180.0,
                            ) {
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
                        theme::status_chip(
                            ui,
                            "BrainFlow: set board id (0 = synthetic); optional serial port for hardware.",
                            theme::Intent::Warning,
                        );
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
                unsaved = true;
            }
            if state.config.device.backend != last_device_backend {
                if runtime
                    .block_on(state.config_store.save(&state.config))
                    .is_err()
                {
                    tracing::warn!("Failed to persist device backend");
                }
            }
            ui.add_space(8.0);


    if unsaved { true } else { false }
}
