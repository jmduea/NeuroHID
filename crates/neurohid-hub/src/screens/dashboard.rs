//! # Dashboard Screen
//!
//! The main overview screen. Shows service status, device info, signal quality,
//! and quick controls for starting/stopping the service.

use eframe::egui;

use crate::state::HubState;
use crate::service_manager::ServiceManager;

pub struct DashboardScreen;

impl DashboardScreen {
    pub fn new() -> Self {
        Self
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &HubState,
        service_manager: &mut ServiceManager,
        runtime: &tokio::runtime::Runtime,
    ) {
        ui.heading("Dashboard");
        ui.add_space(16.0);

        let snap = &state.service_snapshot;

        // Top row: Service + Device cards
        ui.columns(2, |cols| {
            // Service status card
            cols[0].group(|ui| {
                ui.heading("Service");
                ui.add_space(8.0);

                let (color, text) = if snap.running {
                    (egui::Color32::GREEN, "Running")
                } else {
                    (egui::Color32::GRAY, "Stopped")
                };
                ui.horizontal(|ui| {
                    ui.colored_label(color, "●");
                    ui.label(text);
                });

                if snap.running {
                    let mins = snap.uptime_secs / 60;
                    let secs = snap.uptime_secs % 60;
                    ui.label(format!("Uptime: {}:{:02}", mins, secs));
                }

                ui.add_space(8.0);

                if snap.running {
                    if ui.button("Stop Service").clicked() {
                        service_manager.stop();
                    }
                } else {
                    let has_profile = state.active_profile_id.is_some();
                    ui.add_enabled_ui(has_profile, |ui| {
                        if ui.button("Start Service").clicked() {
                            if let Some(profile_id) = &state.active_profile_id {
                                service_manager.start(
                                    runtime,
                                    state.config.clone(),
                                    state.profile_store.clone(),
                                    profile_id.clone(),
                                );
                            }
                        }
                    });
                    if !has_profile {
                        ui.label(
                            egui::RichText::new("Create a profile first")
                                .small()
                                .color(egui::Color32::YELLOW),
                        );
                    }
                }

                if let Some(err) = service_manager.last_error() {
                    ui.colored_label(egui::Color32::RED, err);
                }

                // Show task failure details when the service stopped due to an error
                if let Some((task, error)) = &snap.task_error {
                    ui.add_space(8.0);
                    egui::Frame::group(ui.style())
                        .fill(egui::Color32::from_rgb(60, 20, 20))
                        .show(ui, |ui| {
                            ui.colored_label(
                                egui::Color32::RED,
                                format!("Service stopped: {} task failed", task),
                            );
                            ui.label(
                                egui::RichText::new(error)
                                    .small()
                                    .color(egui::Color32::LIGHT_RED),
                            );
                            if let Some(hint) = task_error_hint(error) {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(hint)
                                        .small()
                                        .color(egui::Color32::YELLOW),
                                );
                            }
                        });
                }
            });

            // Device card
            cols[1].group(|ui| {
                ui.heading("Device");
                ui.add_space(8.0);

                if snap.device_connected {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::GREEN, "●");
                        ui.label("Connected");
                    });
                    if let Some(name) = &snap.device_name {
                        ui.label(format!("Name: {}", name));
                    }
                    if let Some(battery) = snap.device_battery {
                        ui.label(format!("Battery: {}%", battery));
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::GRAY, "●");
                        ui.label("No device connected");
                    });
                    if !snap.running {
                        ui.label(
                            egui::RichText::new("Start the service to connect")
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                }
            });
        });

        ui.add_space(16.0);

        // Middle row: Signal quality + Error rate
        ui.columns(2, |cols| {
            // Signal quality
            cols[0].group(|ui| {
                ui.heading("Signal Quality");
                ui.add_space(8.0);

                let quality = snap.signal_quality;
                let quality_color = if quality > 0.7 {
                    egui::Color32::GREEN
                } else if quality > 0.5 {
                    egui::Color32::YELLOW
                } else {
                    egui::Color32::RED
                };

                ui.add(
                    egui::ProgressBar::new(quality)
                        .text(format!("{:.0}%", quality * 100.0))
                        .fill(quality_color),
                );

                // Per-channel bars would go here with real device data
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("5-channel average")
                        .small()
                        .color(egui::Color32::GRAY),
                );
            });

            // Error rate
            cols[1].group(|ui| {
                ui.heading("Error Rate");
                ui.add_space(8.0);

                let error_rate = state.error_rate();
                let error_color = if error_rate < 10.0 {
                    egui::Color32::GREEN
                } else if error_rate < 30.0 {
                    egui::Color32::YELLOW
                } else {
                    egui::Color32::RED
                };

                ui.colored_label(error_color, format!("{:.1}%", error_rate));
                ui.label(format!(
                    "{} errors / {} actions",
                    snap.errors_detected, snap.actions_emitted
                ));
            });
        });

        ui.add_space(16.0);

        // Bottom row: Counters
        ui.group(|ui| {
            ui.heading("Activity");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(format!("Actions Emitted: {}", snap.actions_emitted));
                ui.separator();
                ui.label(format!("Errors Detected: {}", snap.errors_detected));
                ui.separator();
                if let Some(name) = &snap.active_profile_name {
                    ui.label(format!("Profile: {}", name));
                } else if let Some(id) = &state.active_profile_id {
                    ui.label(format!("Profile: {}", id));
                }
            });
        });
    }
}

/// Returns a platform-specific remediation hint for known error patterns.
fn task_error_hint(error: &str) -> Option<&'static str> {
    let lower = error.to_lowercase();

    if lower.contains("permission denied") || lower.contains("access denied") {
        if cfg!(target_os = "linux") {
            return Some("Hint: Create a udev rule for /dev/uinput access, then add your user to the 'input' group. See the service log for full instructions.");
        } else if cfg!(target_os = "macos") {
            return Some("Hint: Grant Accessibility access in System Settings > Privacy & Security > Accessibility");
        } else {
            return Some("Hint: Try running with elevated permissions");
        }
    }

    if lower.contains("device not found") || lower.contains("no device") || lower.contains("not connected") {
        return Some("Hint: Ensure your EEG device is powered on and paired via Bluetooth");
    }

    if lower.contains("connection refused") || lower.contains("connect error") {
        return Some("Hint: Ensure the Python ML service is running (python -m neurohid_ml)");
    }

    None
}
