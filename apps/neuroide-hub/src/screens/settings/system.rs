use crate::theme;
use eframe::egui;
use neurohid_core::extension_registry::{ExtensionRegistry, default_extension_paths};

pub fn render(
    ui: &mut egui::Ui,
    state: &mut crate::state::HubState,
    runtime: &tokio::runtime::Runtime,
) -> bool {
    let mut unsaved = false;

    ui.label(egui::RichText::new("Runtime orchestration").small().weak());
    // Service settings
    let changed = egui::CollapsingHeader::new("Service")
        .default_open(false)
        .show(ui, |ui| {
            let mut changed = false;
            let cfg = &mut state.config.service;

            ui.horizontal(|ui| {
                ui.label("Run:");
                let current_mode = cfg.runtime_mode.clone();
                let options: Vec<_> = neurohid_types::config::ServiceRuntimeMode::ALL
                    .iter()
                    .map(|m| m.ui_label().to_string())
                    .collect();
                let options_ref: Vec<&str> = options.iter().map(String::as_str).collect();
                let mut selected = neurohid_types::config::ServiceRuntimeMode::ALL
                    .iter()
                    .position(|m| {
                        std::mem::discriminant(m) == std::mem::discriminant(&cfg.runtime_mode)
                    })
                    .unwrap_or(0);
                if theme::select_index(
                    ui,
                    "settings_service_runtime_mode",
                    &mut selected,
                    &options_ref,
                    200.0,
                ) {
                    cfg.runtime_mode = match selected {
                        0 => neurohid_types::config::ServiceRuntimeMode::Embedded,
                        _ => neurohid_types::config::ServiceRuntimeMode::External,
                    };
                }
                if cfg.runtime_mode != current_mode {
                    changed = true;
                }
            });

            if cfg.runtime_mode == neurohid_types::config::ServiceRuntimeMode::External {
                ui.horizontal(|ui| {
                    ui.label("IPC endpoint:");
                    if theme::text_input(
                        ui,
                        "settings_ipc_endpoint",
                        &mut cfg.ipc_endpoint,
                        "neurohid.control.v3",
                        220.0,
                    ) {
                        changed = true;
                    }
                });
                theme::status_chip(
                    ui,
                    "Run in background requires neurohid-service running separately",
                    theme::Intent::Warning,
                );
            } else {
                theme::status_chip(
                    ui,
                    "Run in Hub runs the runtime inside this window",
                    theme::Intent::Muted,
                );
            }

            ui.horizontal(|ui| {
                ui.label("Log level:");
                let levels = ["trace", "debug", "info", "warn", "error"];
                let current = levels.iter().position(|&l| l == cfg.log_level).unwrap_or(2);
                let mut selected = current;
                let _ =
                    theme::select_index(ui, "settings_log_level", &mut selected, &levels, 140.0);
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
        unsaved = true;
    }
    ui.add_space(8.0);

    let last_outlet_extension = state.config.outlet.extension_name.clone();
    // Outlet settings
    let changed = egui::CollapsingHeader::new("Outlets")
        .default_open(false)
        .show(ui, |ui| {
            let mut changed = false;
            let cfg = &mut state.config.outlet;

            // Outlet: built-in or extension by name
            ui.horizontal(|ui| {
                ui.label("Outlet:");
                let mut reg = ExtensionRegistry::new(default_extension_paths());
                let _ = reg.scan();
                let ext_names: Vec<String> =
                    reg.list_outlets().into_iter().map(|e| e.name).collect();
                let options: Vec<String> = std::iter::once("Built-in".to_string())
                    .chain(ext_names.clone())
                    .collect();
                let options_ref: Vec<&str> = options.iter().map(String::as_str).collect();
                let current = cfg.extension_name.as_deref().unwrap_or("");
                let mut selected = options
                    .iter()
                    .position(|s| s.as_str() == current || (current.is_empty() && s == "Built-in"))
                    .unwrap_or(0);
                if theme::select_index(
                    ui,
                    "settings_outlet_extension",
                    &mut selected,
                    &options_ref,
                    180.0,
                ) {
                    cfg.extension_name = if selected == 0 {
                        None
                    } else if let Some(name) = options.get(selected) {
                        Some(name.clone())
                    } else {
                        None
                    };
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Enable outlets:");
                if theme::toggle_switch(ui, "settings_outlets_enabled", &mut cfg.enabled) {
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Publish samples:");
                if theme::toggle_switch(ui, "settings_publish_samples", &mut cfg.publish_samples) {
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Publish features:");
                if theme::toggle_switch(ui, "settings_publish_features", &mut cfg.publish_features)
                {
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Publish actions:");
                if theme::toggle_switch(ui, "settings_publish_actions", &mut cfg.publish_actions) {
                    changed = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Publish markers:");
                if theme::toggle_switch(ui, "settings_publish_markers", &mut cfg.publish_markers) {
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
        unsaved = true;
    }
    if state.config.outlet.extension_name != last_outlet_extension {
        if runtime
            .block_on(state.config_store.save(&state.config))
            .is_err()
        {
            tracing::warn!("Failed to persist outlet extension");
        }
    }
    ui.add_space(8.0);

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
                if theme::drag_value(ui, &mut cfg.sustained_duration_secs, 5..=3600, 1.0, None) {
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
        unsaved = true;
    }
    ui.add_space(8.0);

    ui.label(
        egui::RichText::new("Persistence & retention")
            .small()
            .weak(),
    );
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
                if theme::drag_value(ui, &mut cfg.session_log_retention_days, 1..=365, 1.0, None) {
                    changed = true;
                }
            });

            changed
        });
    if changed.body_returned == Some(true) {
        unsaved = true;
    }

    unsaved
}
