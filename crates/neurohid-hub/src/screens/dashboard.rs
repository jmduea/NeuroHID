//! # Dashboard Screen
//!
//! The main overview screen. Shows service status, device info, signal quality,
//! and quick controls for starting/stopping the service.

use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use eframe::egui;
use neurohid_storage::ProfileStore;
use neurohid_types::{
    config::{ServiceRuntimeMode, UiMode},
    control::RuntimeModeState,
    profile::ProfileId,
};

use crate::service_manager::ServiceManager;
use crate::state::HubState;

pub struct DashboardScreen {
    train_stage_status: Option<TrainStageStatus>,
    train_stage_output: String,
    train_stage_rx: Option<Receiver<TrainStageResult>>,
}

enum TrainStageStatus {
    Running(String),
    Success(String),
    Error(String),
}

struct TrainStageResult {
    success: bool,
    message: String,
    output: String,
}

impl DashboardScreen {
    pub fn new() -> Self {
        Self {
            train_stage_status: None,
            train_stage_output: String::new(),
            train_stage_rx: None,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &HubState,
        service_manager: &mut ServiceManager,
        runtime: &tokio::runtime::Runtime,
    ) {
        self.poll_train_stage_result();

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
                ui.label(
                    egui::RichText::new(format!("Mode: {}", state.config.service.runtime_mode))
                        .small()
                        .color(egui::Color32::GRAY),
                );

                if snap.running {
                    let mins = snap.uptime_secs / 60;
                    let secs = snap.uptime_secs % 60;
                    ui.label(format!("Uptime: {}:{:02}", mins, secs));
                }

                ui.horizontal(|ui| {
                    let (output_color, output_text) = if snap.output_enabled {
                        (egui::Color32::GREEN, "Output enabled")
                    } else {
                        (egui::Color32::YELLOW, "Output paused")
                    };
                    ui.colored_label(output_color, "●");
                    ui.label(output_text);
                });

                ui.horizontal(|ui| {
                    let (profile_color, profile_text) = if snap.profile_ready {
                        (egui::Color32::GREEN, "Profile ready")
                    } else {
                        (egui::Color32::YELLOW, "Profile not calibrated")
                    };
                    ui.colored_label(profile_color, "●");
                    ui.label(profile_text);
                });

                ui.horizontal(|ui| {
                    let (decoder_color, decoder_text) = if snap.decoder_ready {
                        (egui::Color32::GREEN, "Decoder ready")
                    } else {
                        (egui::Color32::YELLOW, "Decoder unavailable")
                    };
                    ui.colored_label(decoder_color, "●");
                    ui.label(decoder_text);
                });

                ui.horizontal(|ui| {
                    let (bridge_color, bridge_text) = if snap.ml_bridge_connected {
                        if snap.ml_bridge_stalled {
                            (egui::Color32::YELLOW, "ML bridge stalled")
                        } else {
                            (egui::Color32::GREEN, "ML bridge connected")
                        }
                    } else {
                        (egui::Color32::GRAY, "ML bridge disconnected")
                    };
                    ui.colored_label(bridge_color, "●");
                    ui.label(bridge_text);
                });

                let (mode_color, mode_text) = match snap.runtime_mode_state {
                    RuntimeModeState::Full => (egui::Color32::GREEN, "Runtime mode: full"),
                    RuntimeModeState::Fallback => (egui::Color32::YELLOW, "Runtime mode: fallback"),
                    RuntimeModeState::Degraded => (egui::Color32::RED, "Runtime mode: degraded"),
                };
                ui.colored_label(mode_color, mode_text);

                if let Some(version) = &snap.decoder_model_version {
                    ui.label(
                        egui::RichText::new(format!("Model: {}", version))
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                }
                if let Some(model_kind) = &snap.fallback_model_kind {
                    ui.label(
                        egui::RichText::new(format!("Active model path: {}", model_kind))
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                }

                let capability_text = if snap.enabled_capabilities.is_empty() {
                    "Enabled capabilities: none".to_string()
                } else {
                    format!(
                        "Enabled capabilities: {}",
                        snap.enabled_capabilities.join(", ")
                    )
                };
                ui.label(
                    egui::RichText::new(capability_text)
                        .small()
                        .color(egui::Color32::GRAY),
                );
                if let Some(message) = &snap.limited_capabilities_message {
                    let color = if snap.runtime_mode_state == RuntimeModeState::Degraded {
                        egui::Color32::RED
                    } else {
                        egui::Color32::YELLOW
                    };
                    ui.colored_label(color, message);
                }

                ui.label(
                    egui::RichText::new(format!(
                        "Signal latency: last {} us | p95 {} us",
                        snap.signal_latency_last_us, snap.signal_latency_p95_us
                    ))
                    .small()
                    .color(egui::Color32::GRAY),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Decode latency: last {} us | p95 {} us",
                        snap.decode_latency_last_us, snap.decode_latency_p95_us
                    ))
                    .small()
                    .color(egui::Color32::GRAY),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "Action latency: last {} us | p95 {} us",
                        snap.action_latency_last_us, snap.action_latency_p95_us
                    ))
                    .small()
                    .color(egui::Color32::GRAY),
                );

                if snap.latency_degraded {
                    let message = snap
                        .latency_alert_message
                        .clone()
                        .unwrap_or_else(|| "Latency thresholds exceeded".to_string());
                    ui.colored_label(egui::Color32::RED, message);
                }

                ui.add_space(8.0);

                if snap.running {
                    let stop_label = if state.config.service.runtime_mode
                        == ServiceRuntimeMode::External
                    {
                        "Request Shutdown"
                    } else {
                        "Stop Service"
                    };
                    if ui.button(stop_label).clicked() {
                        service_manager.stop();
                    }

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Reload Model").clicked() {
                            service_manager.reload_model();
                        }
                        let can_promote = snap.profile_ready;
                        let promote =
                            ui.add_enabled(can_promote, egui::Button::new("Promote Candidate"));
                        if promote.clicked() {
                            service_manager.promote_candidate_model();
                        }
                    });
                    if !snap.profile_ready {
                        ui.label(
                            egui::RichText::new(
                                "Candidate promotion requires a calibrated active profile",
                            )
                            .small()
                            .color(egui::Color32::GRAY),
                        );
                    }

                    if state.config.ui.mode == UiMode::Advanced {
                        ui.add_space(6.0);
                        let has_profile = state.active_profile_id.is_some();
                        let train_button = ui.add_enabled(
                            self.train_stage_rx.is_none() && has_profile,
                            egui::Button::new("Train + Stage Candidate"),
                        );
                        if train_button.clicked() {
                            self.start_train_stage_job(state);
                        }
                        if !has_profile {
                            ui.label(
                                egui::RichText::new(
                                    "Training requires an active profile selection",
                                )
                                .small()
                                .color(egui::Color32::GRAY),
                            );
                        }
                    }
                } else {
                    let start_label = if state.config.service.runtime_mode
                        == ServiceRuntimeMode::External
                    {
                        "Probe External Service"
                    } else {
                        "Start Service"
                    };
                    if ui.button(start_label).clicked() {
                        service_manager.start(
                            runtime,
                            state.config.clone(),
                            Some(state.profile_store.clone()),
                            state.active_profile_id.clone(),
                        );
                    }
                    if state.config.service.runtime_mode == ServiceRuntimeMode::External {
                        ui.label(
                            egui::RichText::new(
                                "External mode requires `neurohid-service --control-port` to be running.",
                            )
                            .small()
                            .color(egui::Color32::YELLOW),
                        );
                    }
                    if state.active_profile_id.is_none() {
                        ui.label(
                            egui::RichText::new("No profile selected — running in discovery mode")
                                .small()
                                .color(egui::Color32::YELLOW),
                        );
                    }
                }

                if let Some(status) = &self.train_stage_status {
                    let (color, text) = match status {
                        TrainStageStatus::Running(msg) => (egui::Color32::YELLOW, msg.as_str()),
                        TrainStageStatus::Success(msg) => (egui::Color32::GREEN, msg.as_str()),
                        TrainStageStatus::Error(msg) => (egui::Color32::RED, msg.as_str()),
                    };
                    ui.add_space(6.0);
                    ui.colored_label(color, text);
                }

                if !self.train_stage_output.is_empty() {
                    ui.add_space(4.0);
                    ui.collapsing("Train + Stage Output", |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(140.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.train_stage_output)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(f32::INFINITY),
                                );
                            });
                    });
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

                let total_streams = snap.discovered_streams.len();
                let connected_streams = snap
                    .discovered_streams
                    .iter()
                    .filter(|s| s.connected)
                    .count();

                if connected_streams > 0 {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::GREEN, "●");
                        ui.label(format!(
                            "{} connected, {} available",
                            connected_streams, total_streams
                        ));

                        // Show battery if any connected device reports it
                        if let Some(battery) = snap.device_battery {
                            let bat_color = if battery > 50 {
                                egui::Color32::GREEN
                            } else if battery > 20 {
                                egui::Color32::YELLOW
                            } else {
                                egui::Color32::RED
                            };
                            ui.colored_label(bat_color, format!("{}%", battery));
                        }
                    });
                    // List connected stream names with per-stream quality
                    for s in snap.discovered_streams.iter().filter(|s| s.connected) {
                        ui.horizontal(|ui| {
                            let mut label = format!("  {} ({})", s.name, s.stream_type);
                            if let Some(bat) = s.battery_percent {
                                label.push_str(&format!(" | Bat: {}%", bat));
                            }
                            ui.label(egui::RichText::new(label).small());
                        });
                    }
                } else if total_streams > 0 {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::YELLOW, "●");
                        ui.label(format!("{} stream(s) available", total_streams));
                    });
                    ui.label(
                        egui::RichText::new("Go to Devices to connect")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                } else {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::GRAY, "●");
                        ui.label("No streams found");
                    });
                    if !snap.running {
                        ui.label(
                            egui::RichText::new("Start the service to discover streams")
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

    fn start_train_stage_job(&mut self, state: &HubState) {
        let Some(profile_id) = state.active_profile_id.clone() else {
            self.train_stage_status = Some(TrainStageStatus::Error(
                "No active profile selected".to_string(),
            ));
            return;
        };

        self.train_stage_status = Some(TrainStageStatus::Running(format!(
            "Training candidate from session logs for profile '{}'",
            profile_id
        )));
        self.train_stage_output.clear();

        let profile_store = state.profile_store.clone();
        let (tx, rx) = mpsc::channel();
        self.train_stage_rx = Some(rx);

        std::thread::spawn(move || {
            let result = run_train_stage_candidate_job(profile_store, profile_id);
            let _ = tx.send(result);
        });
    }

    fn poll_train_stage_result(&mut self) {
        let Some(rx) = &self.train_stage_rx else {
            return;
        };

        match rx.try_recv() {
            Ok(result) => {
                self.train_stage_status = Some(if result.success {
                    TrainStageStatus::Success(result.message)
                } else {
                    TrainStageStatus::Error(result.message)
                });
                self.train_stage_output = result.output;
                self.train_stage_rx = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.train_stage_status = Some(TrainStageStatus::Error(
                    "Training job disconnected unexpectedly".to_string(),
                ));
                self.train_stage_rx = None;
            }
        }
    }
}

fn run_train_stage_candidate_job(
    profile_store: ProfileStore,
    profile_id: ProfileId,
) -> TrainStageResult {
    let mut output = String::new();
    let work_dir = std::env::temp_dir().join(format!(
        "neurohid_candidate_{}_{}",
        profile_id,
        neurohid_types::now_micros()
    ));
    let session_dir = work_dir.join("sessions");
    let candidate_dir = work_dir.join("candidate");

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            return TrainStageResult {
                success: false,
                message: format!("Failed to initialize training runtime: {}", error),
                output,
            };
        }
    };

    let exported = match runtime
        .block_on(profile_store.export_training_session_logs_to_dir(&profile_id, &session_dir))
    {
        Ok(exported) => exported,
        Err(error) => {
            return TrainStageResult {
                success: false,
                message: format!("Failed to export session logs: {}", error),
                output,
            };
        }
    };

    output.push_str(&format!(
        "Exported {} session log(s) to {}\n",
        exported,
        session_dir.display()
    ));

    if exported == 0 {
        return TrainStageResult {
            success: false,
            message: format!(
                "No recorded training sessions found for profile '{}'",
                profile_id
            ),
            output,
        };
    }

    let model_version = format!("candidate-{}", neurohid_types::now_micros());
    let trainer_output =
        match run_python_candidate_trainer(&session_dir, &candidate_dir, &model_version) {
            Ok(text) => text,
            Err(error) => {
                output.push_str(&error);
                return TrainStageResult {
                    success: false,
                    message: "Candidate training failed".to_string(),
                    output,
                };
            }
        };
    output.push_str(&trainer_output);

    if let Err(error) = runtime
        .block_on(profile_store.import_decoder_candidate_from_dir(&profile_id, &candidate_dir))
    {
        output.push_str(&format!(
            "\nFailed importing candidate artifacts: {}\n",
            error
        ));
        return TrainStageResult {
            success: false,
            message: "Candidate import failed".to_string(),
            output,
        };
    }
    output.push_str("Imported candidate artifacts into encrypted profile storage\n");

    if let Err(error) = std::fs::remove_dir_all(&work_dir) {
        output.push_str(&format!(
            "Cleanup warning for {}: {}\n",
            work_dir.display(),
            error
        ));
    }

    TrainStageResult {
        success: true,
        message: format!(
            "Candidate staged for profile '{}' from {} session(s). Click Promote Candidate.",
            profile_id, exported
        ),
        output,
    }
}

fn run_python_candidate_trainer(
    session_dir: &Path,
    output_dir: &Path,
    model_version: &str,
) -> std::result::Result<String, String> {
    let args = vec![
        "run".to_string(),
        "neurohid-ml".to_string(),
        "train-candidate".to_string(),
        "--session-dir".to_string(),
        session_dir.display().to_string(),
        "--output-dir".to_string(),
        output_dir.display().to_string(),
        "--model-version".to_string(),
        model_version.to_string(),
    ];

    let mut cmd = Command::new("uv");
    cmd.current_dir("python").args(&args);

    let output = cmd
        .output()
        .map_err(|error| format!("Failed to execute 'uv' for candidate training: {}\n", error))?;

    let mut text = format!("$ uv {}\n", args.join(" "));
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    if output.status.success() {
        return Ok(text);
    }

    Err(format!(
        "{text}\nTrainer exited with status {}\n",
        output.status
    ))
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

    if lower.contains("device not found")
        || lower.contains("no device")
        || lower.contains("not connected")
    {
        return Some("Hint: Ensure your EEG device is powered on and paired via Bluetooth");
    }

    if lower.contains("connection refused") || lower.contains("connect error") {
        return Some(
            "Hint: Ensure the Python ML service is running (uv run --directory python neurohid-ml bridge)",
        );
    }

    None
}
