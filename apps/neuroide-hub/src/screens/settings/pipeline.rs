use crate::theme;
use eframe::egui;
use neurohid_core::extension_registry::{ExtensionRegistry, default_extension_paths};
use neurohid_types::config::SignalConfig;

pub fn signal_preset_name(config: &SignalConfig) -> &'static str {
    if matches_signal_preset(
        config, 1024, true, 60.0, true, 0.5, 45.0, true, 100.0, 500, 50,
    ) {
        "Balanced"
    } else if matches_signal_preset(
        config, 1024, true, 60.0, true, 1.0, 30.0, true, 80.0, 400, 40,
    ) {
        "Focus"
    } else if matches_signal_preset(
        config, 1024, false, 60.0, false, 0.5, 45.0, false, 100.0, 500, 50,
    ) {
        "Raw"
    } else {
        "Custom"
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Preset matcher compares compact scalar config"
)]
fn matches_signal_preset(
    config: &neurohid_types::config::SignalConfig,
    buffer_size_samples: usize,
    notch_filter_enabled: bool,
    notch_filter_hz: f32,
    bandpass_filter_enabled: bool,
    bandpass_low_hz: f32,
    bandpass_high_hz: f32,
    artifact_rejection_enabled: bool,
    artifact_threshold_uv: f32,
    feature_window_ms: u32,
    feature_step_ms: u32,
) -> bool {
    const EPS: f32 = 0.000_1;
    config.buffer_size_samples == buffer_size_samples
        && config.notch_filter_enabled == notch_filter_enabled
        && (config.notch_filter_hz - notch_filter_hz).abs() <= EPS
        && config.bandpass_filter_enabled == bandpass_filter_enabled
        && (config.bandpass_low_hz - bandpass_low_hz).abs() <= EPS
        && (config.bandpass_high_hz - bandpass_high_hz).abs() <= EPS
        && config.artifact_rejection_enabled == artifact_rejection_enabled
        && (config.artifact_threshold_uv - artifact_threshold_uv).abs() <= EPS
        && config.feature_window_ms == feature_window_ms
        && config.feature_step_ms == feature_step_ms
}

pub fn apply_signal_preset(config: &mut SignalConfig, preset: &str) -> bool {
    let previous = config.clone();
    match preset {
        "Balanced" => {
            *config = neurohid_types::config::SignalConfig::default();
        }
        "Focus" => {
            *config = neurohid_types::config::SignalConfig::default();
            config.bandpass_low_hz = 1.0;
            config.bandpass_high_hz = 30.0;
            config.artifact_threshold_uv = 80.0;
            config.feature_window_ms = 400;
            config.feature_step_ms = 40;
        }
        "Raw" => {
            *config = neurohid_types::config::SignalConfig::default();
            config.notch_filter_enabled = false;
            config.bandpass_filter_enabled = false;
            config.artifact_rejection_enabled = false;
        }
        _ => {}
    }
    *config != previous
}

pub fn render(
    ui: &mut egui::Ui,
    state: &mut crate::state::HubState,
    service_manager: &crate::service_manager::ServiceManager,
    runtime: &tokio::runtime::Runtime,
) -> bool {
    let mut unsaved = false;
    let mut signal_changed_this_frame = false;

    ui.label(
        egui::RichText::new("Signal & action pipeline")
            .small()
            .weak(),
    );
    let last_signal_extension = state.config.signal.extension_name.clone();
    // Signal settings
    let changed = egui::CollapsingHeader::new("Signal Processing")
        .default_open(false)
        .show(ui, |ui| {
            let mut changed = false;
            let cfg = &mut state.config.signal;

            // Signal preprocessing: built-in or extension by name
            ui.horizontal(|ui| {
                ui.label("Pipeline:");
                let mut reg = ExtensionRegistry::new(default_extension_paths());
                let _ = reg.scan();
                let ext_names: Vec<String> = reg
                    .list_signal_preprocessors()
                    .into_iter()
                    .map(|e| e.name)
                    .collect();
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
                    "settings_signal_extension",
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

            ui.horizontal_wrapped(|ui| {
                ui.label("Presets:");
                if theme::action_button(ui, "Balanced", true, theme::ButtonTone::Secondary)
                    && apply_signal_preset(cfg, "Balanced")
                {
                    changed = true;
                }
                if theme::action_button(ui, "Focus", true, theme::ButtonTone::Secondary)
                    && apply_signal_preset(cfg, "Focus")
                {
                    changed = true;
                }
                if theme::action_button(ui, "Raw", true, theme::ButtonTone::Secondary)
                    && apply_signal_preset(cfg, "Raw")
                {
                    changed = true;
                }
            });
            theme::status_chip(
                ui,
                &format!("Current preset: {}", signal_preset_name(cfg)),
                if signal_preset_name(cfg) == "Custom" {
                    theme::Intent::Muted
                } else {
                    theme::Intent::Info
                },
            );

            ui.horizontal_wrapped(|ui| {
                let notch_label = if cfg.notch_filter_enabled {
                    format!("Notch {:.0}Hz", cfg.notch_filter_hz)
                } else {
                    "Notch off".to_string()
                };
                theme::status_chip(
                    ui,
                    &notch_label,
                    if cfg.notch_filter_enabled {
                        theme::Intent::Info
                    } else {
                        theme::Intent::Muted
                    },
                );
                let bandpass_label = if cfg.bandpass_filter_enabled {
                    format!(
                        "Bandpass {:.1}-{:.1}Hz",
                        cfg.bandpass_low_hz, cfg.bandpass_high_hz
                    )
                } else {
                    "Bandpass off".to_string()
                };
                theme::status_chip(
                    ui,
                    &bandpass_label,
                    if cfg.bandpass_filter_enabled {
                        theme::Intent::Info
                    } else {
                        theme::Intent::Muted
                    },
                );
                let artifact_label = if cfg.artifact_rejection_enabled {
                    format!("Artifact {:.1}uV", cfg.artifact_threshold_uv)
                } else {
                    "Artifact off".to_string()
                };
                theme::status_chip(
                    ui,
                    &artifact_label,
                    if cfg.artifact_rejection_enabled {
                        theme::Intent::Info
                    } else {
                        theme::Intent::Muted
                    },
                );
            });

            ui.horizontal(|ui| {
                ui.label("Buffer samples:");
                if theme::drag_value(ui, &mut cfg.buffer_size_samples, 128..=16_384, 1.0, None) {
                    changed = true;
                }
            });

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
                if theme::drag_value(ui, &mut cfg.notch_filter_hz, 45.0..=65.0, 1.0, None) {
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
                if theme::drag_value(ui, &mut cfg.bandpass_low_hz, 0.1..=10.0, 0.1, None) {
                    changed = true;
                }
                ui.label("High Hz:");
                if theme::drag_value(ui, &mut cfg.bandpass_high_hz, 10.0..=100.0, 1.0, None) {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Window (ms):");
                if theme::drag_value(ui, &mut cfg.feature_window_ms, 100..=2000, 1.0, None) {
                    changed = true;
                }
                ui.label("Step (ms):");
                if theme::drag_value(ui, &mut cfg.feature_step_ms, 10..=500, 1.0, None) {
                    changed = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Artifact rejection:");
                if theme::toggle_switch(
                    ui,
                    "settings_artifact_rejection_enabled",
                    &mut cfg.artifact_rejection_enabled,
                ) {
                    changed = true;
                }
                ui.label("Threshold (uV):");
                if theme::drag_value(
                    ui,
                    &mut cfg.artifact_threshold_uv,
                    10.0..=1_000.0,
                    1.0,
                    None,
                ) {
                    changed = true;
                }
            });
            theme::status_chip(
                ui,
                "Hot-updates apply live in embedded and external runtime modes",
                theme::Intent::Info,
            );

            changed
        });
    if changed.body_returned == Some(true) {
        unsaved = true;
        signal_changed_this_frame = true;
    }
    if state.config.signal.extension_name != last_signal_extension {
        if runtime
            .block_on(state.config_store.save(&state.config))
            .is_err()
        {
            tracing::warn!("Failed to persist signal extension");
        }
    }
    ui.add_space(8.0);

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
                if theme::drag_value(ui, &mut cfg.action_debounce_ms, 0..=1000, 1.0, None) {
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
        unsaved = true;
    }
    ui.add_space(8.0);

    let last_decoder_extension = state.config.decoder.extension_name.clone();
    // Decoder settings (advanced)
    let changed = egui::CollapsingHeader::new("Decoder (Advanced)")
        .default_open(false)
        .show(ui, |ui| {
            let mut changed = false;
            let cfg = &mut state.config.decoder;

            // Decoder: built-in or extension by name
            ui.horizontal(|ui| {
                ui.label("Decoder:");
                let mut reg = ExtensionRegistry::new(default_extension_paths());
                let _ = reg.scan();
                let ext_names: Vec<String> =
                    reg.list_decoders().into_iter().map(|e| e.name).collect();
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
                    "settings_decoder_extension",
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
                ui.label("Learning rate:");
                if theme::drag_value(ui, &mut cfg.learning_rate, 1e-5..=1e-2, 1e-5, None) {
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
        unsaved = true;
    }
    if state.config.decoder.extension_name != last_decoder_extension {
        if runtime
            .block_on(state.config_store.save(&state.config))
            .is_err()
        {
            tracing::warn!("Failed to persist decoder extension");
        }
    }
    ui.add_space(8.0);

    if signal_changed_this_frame {
        service_manager.update_signal_config(state.config.signal.clone());
    }

    unsaved
}

#[cfg(test)]
mod tests {
    use super::{apply_signal_preset, signal_preset_name};
    #[test]
    fn signal_preset_name_identifies_defaults_as_balanced() {
        let config = neurohid_types::config::SignalConfig::default();
        assert_eq!(signal_preset_name(&config), "Balanced");
    }

    #[test]
    fn applying_focus_signal_preset_updates_expected_fields() {
        let mut config = neurohid_types::config::SignalConfig::default();
        assert!(apply_signal_preset(&mut config, "Focus"));
        assert_eq!(signal_preset_name(&config), "Focus");
        assert!(config.notch_filter_enabled);
        assert!(config.bandpass_filter_enabled);
        assert!(config.artifact_rejection_enabled);
        assert_eq!(config.bandpass_low_hz, 1.0);
        assert_eq!(config.bandpass_high_hz, 30.0);
        assert_eq!(config.artifact_threshold_uv, 80.0);
        assert_eq!(config.feature_window_ms, 400);
        assert_eq!(config.feature_step_ms, 40);
    }

    #[test]
    fn applying_raw_signal_preset_disables_preprocessing() {
        let mut config = neurohid_types::config::SignalConfig::default();
        assert!(apply_signal_preset(&mut config, "Raw"));
        assert_eq!(signal_preset_name(&config), "Raw");
        assert!(!config.notch_filter_enabled);
        assert!(!config.bandpass_filter_enabled);
        assert!(!config.artifact_rejection_enabled);
    }
}
