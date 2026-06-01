use crate::theme;
use eframe::egui;
use neurohid_types::config::DeviceHealthProblemConfig;

pub fn device_health_preset_name(config: &DeviceHealthProblemConfig) -> &'static str {
    if matches_device_health_preset(config, 30, 15, 0.65, 0.50) {
        "Conservative"
    } else if matches_device_health_preset(config, 20, 10, 0.50, 0.35) {
        "Balanced"
    } else if matches_device_health_preset(config, 15, 5, 0.40, 0.25) {
        "Aggressive"
    } else {
        "Custom"
    }
}

fn matches_device_health_preset(
    config: &neurohid_types::config::DeviceHealthProblemConfig,
    battery_low: u8,
    battery_critical: u8,
    quality_warning: f32,
    quality_critical: f32,
) -> bool {
    const EPS: f32 = 0.000_1;
    config.battery_low_threshold_pct == battery_low
        && config.battery_critical_threshold_pct == battery_critical
        && (config.quality_warning_threshold - quality_warning).abs() <= EPS
        && (config.quality_critical_threshold - quality_critical).abs() <= EPS
}

pub fn render(
    ui: &mut egui::Ui,
    state: &mut crate::state::HubState,
    _runtime: &tokio::runtime::Runtime,
) -> bool {
    let mut unsaved = false;

    ui.label(egui::RichText::new("UI experience").small().weak());
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
                if theme::select_index(ui, "settings_ui_theme_mode", &mut selected, &options, 160.0)
                {
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

            theme::status_chip(
                ui,
                "Visualization docking backend: egui_dock",
                theme::Intent::Muted,
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
                ui.label("Visualization FPS:");
                if theme::drag_value(
                    ui,
                    &mut cfg.visualization_target_fps,
                    5..=60,
                    1.0,
                    Some(" fps"),
                ) {
                    changed = true;
                }
            });
            theme::status_chip(
                ui,
                "Higher FPS improves smoothness but uses more CPU",
                theme::Intent::Muted,
            );

            ui.separator();
            ui.label(
                egui::RichText::new("Problems panel device health")
                    .small()
                    .strong(),
            );

            ui.horizontal_wrapped(|ui| {
                ui.label("Presets:");
                if theme::action_button(ui, "Conservative", true, theme::ButtonTone::Secondary) {
                    cfg.device_health_problems.battery_low_threshold_pct = 30;
                    cfg.device_health_problems.battery_critical_threshold_pct = 15;
                    cfg.device_health_problems.quality_warning_threshold = 0.65;
                    cfg.device_health_problems.quality_critical_threshold = 0.50;
                    changed = true;
                }
                if theme::action_button(ui, "Balanced", true, theme::ButtonTone::Secondary) {
                    cfg.device_health_problems.battery_low_threshold_pct = 20;
                    cfg.device_health_problems.battery_critical_threshold_pct = 10;
                    cfg.device_health_problems.quality_warning_threshold = 0.50;
                    cfg.device_health_problems.quality_critical_threshold = 0.35;
                    changed = true;
                }
                if theme::action_button(ui, "Aggressive", true, theme::ButtonTone::Secondary) {
                    cfg.device_health_problems.battery_low_threshold_pct = 15;
                    cfg.device_health_problems.battery_critical_threshold_pct = 5;
                    cfg.device_health_problems.quality_warning_threshold = 0.40;
                    cfg.device_health_problems.quality_critical_threshold = 0.25;
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Battery warning (%):");
                if theme::drag_value(
                    ui,
                    &mut cfg.device_health_problems.battery_low_threshold_pct,
                    1..=100,
                    1.0,
                    None,
                ) {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Battery critical (%):");
                if theme::drag_value(
                    ui,
                    &mut cfg.device_health_problems.battery_critical_threshold_pct,
                    1..=100,
                    1.0,
                    None,
                ) {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Quality warning threshold:");
                if theme::slider_f32(
                    ui,
                    "settings_device_health_quality_warning_threshold",
                    &mut cfg.device_health_problems.quality_warning_threshold,
                    0.0,
                    1.0,
                    None,
                ) {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Quality critical threshold:");
                if theme::slider_f32(
                    ui,
                    "settings_device_health_quality_critical_threshold",
                    &mut cfg.device_health_problems.quality_critical_threshold,
                    0.0,
                    1.0,
                    None,
                ) {
                    changed = true;
                }
            });

            {
                let health = &mut cfg.device_health_problems;
                let mut normalized = false;

                let warning_battery = health.battery_low_threshold_pct.clamp(1, 100);
                if warning_battery != health.battery_low_threshold_pct {
                    health.battery_low_threshold_pct = warning_battery;
                    normalized = true;
                }
                let critical_max = health.battery_low_threshold_pct.saturating_sub(1).max(1);
                let critical_battery = health.battery_critical_threshold_pct.clamp(1, critical_max);
                if critical_battery != health.battery_critical_threshold_pct {
                    health.battery_critical_threshold_pct = critical_battery;
                    normalized = true;
                }

                let warning_quality = health.quality_warning_threshold.clamp(0.0, 1.0);
                if (warning_quality - health.quality_warning_threshold).abs() > f32::EPSILON {
                    health.quality_warning_threshold = warning_quality;
                    normalized = true;
                }
                let critical_quality = health
                    .quality_critical_threshold
                    .clamp(0.0, health.quality_warning_threshold);
                if (critical_quality - health.quality_critical_threshold).abs() > f32::EPSILON {
                    health.quality_critical_threshold = critical_quality;
                    normalized = true;
                }

                if normalized {
                    changed = true;
                }
            }

            let preset = device_health_preset_name(&cfg.device_health_problems);
            theme::status_chip(
                ui,
                &format!("Current preset: {}", preset),
                if preset == "Custom" {
                    theme::Intent::Muted
                } else {
                    theme::Intent::Info
                },
            );

            theme::status_chip(
                ui,
                "Critical thresholds always clamp below warning thresholds",
                theme::Intent::Muted,
            );

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

            theme::status_chip(
                ui,
                "Advanced mode uses managed Jupyter IDE settings above",
                theme::Intent::Info,
            );
            changed
        });
    if changed.body_returned == Some(true) {
        unsaved = true;
    }
    ui.add_space(8.0);

    unsaved
}
