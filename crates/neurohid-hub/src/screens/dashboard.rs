//! # Dashboard Screen
//!
//! The main overview screen. Shows service status, device info, signal quality,
//! and quick controls for starting/stopping the service.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use eframe::egui;
use egui_async::Bind;
use neurohid_storage::ProfileStore;
use neurohid_types::{
    config::{ServiceRuntimeMode, UiMode},
    control::{RuntimeModeState, TrainerSnapshot},
    profile::ProfileId,
};

use crate::service_manager::ServiceManager;
use crate::state::{HubState, ServiceSnapshot};
use crate::theme;

pub struct DashboardScreen {
    train_stage_status: Option<TrainStageStatus>,
    train_stage_output: String,
    train_stage_task: Bind<TrainStageResult, String>,
    trainer_snapshot: Option<TrainerSnapshot>,
    last_trainer_snapshot_poll: Option<Instant>,
    last_observability_sample: Option<Instant>,
    replay_size_history: MetricHistory,
    training_step_history: MetricHistory,
    policy_loss_history: MetricHistory,
    value_loss_history: MetricHistory,
    entropy_history: MetricHistory,
    candidate_promoted_history: MetricHistory,
    candidate_rejected_history: MetricHistory,
    recent_candidate_outcomes: VecDeque<String>,
    last_candidate_outcome: Option<String>,
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

#[derive(Default)]
struct MetricHistory {
    values: VecDeque<f64>,
}

impl MetricHistory {
    const MAX_POINTS: usize = 120;

    fn clear(&mut self) {
        self.values.clear();
    }

    fn push(&mut self, value: f64) {
        if !value.is_finite() {
            return;
        }
        if self.values.len() == Self::MAX_POINTS {
            let _ = self.values.pop_front();
        }
        self.values.push_back(value);
    }

    fn latest(&self) -> Option<f64> {
        self.values.back().copied()
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn min_max(&self) -> Option<(f64, f64)> {
        let mut iter = self.values.iter().copied();
        let first = iter.next()?;
        let mut min = first;
        let mut max = first;
        for value in iter {
            if value < min {
                min = value;
            }
            if value > max {
                max = value;
            }
        }
        Some((min, max))
    }
}

impl Default for DashboardScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl DashboardScreen {
    const TRAINER_SNAPSHOT_POLL_INTERVAL: Duration = Duration::from_millis(1000);
    const OBSERVABILITY_SAMPLE_INTERVAL: Duration = Duration::from_millis(1000);
    const CANDIDATE_OUTCOME_HISTORY_LIMIT: usize = 8;

    pub fn new() -> Self {
        Self {
            train_stage_status: None,
            train_stage_output: String::new(),
            train_stage_task: Bind::new(true),
            trainer_snapshot: None,
            last_trainer_snapshot_poll: None,
            last_observability_sample: None,
            replay_size_history: MetricHistory::default(),
            training_step_history: MetricHistory::default(),
            policy_loss_history: MetricHistory::default(),
            value_loss_history: MetricHistory::default(),
            entropy_history: MetricHistory::default(),
            candidate_promoted_history: MetricHistory::default(),
            candidate_rejected_history: MetricHistory::default(),
            recent_candidate_outcomes: VecDeque::new(),
            last_candidate_outcome: None,
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

        theme::page_header(
            ui,
            "Dashboard",
            "Mission control for runtime health, model operations, and signal confidence",
        );

        let snap = &state.service_snapshot;
        self.poll_trainer_snapshot(service_manager, snap.running);
        self.sample_trainer_observability(snap);

        let routed_total = snap.routed_eeg_streams
            + snap.routed_motion_streams
            + snap.routed_auxiliary_streams
            + snap.routed_unknown_streams;

        theme::card_frame(ui).show(ui, |ui| {
            ui.heading("Service");
            ui.add_space(8.0);

            ui.horizontal_wrapped(|ui| {
                theme::status_chip(
                    ui,
                    if snap.output_enabled {
                        "Output enabled"
                    } else {
                        "Output paused"
                    },
                    if snap.output_enabled {
                        theme::Intent::Success
                    } else {
                        theme::Intent::Warning
                    },
                );

                theme::status_chip(
                    ui,
                    if snap.profile_ready {
                        "Profile calibrated"
                    } else {
                        "Profile not calibrated"
                    },
                    if snap.profile_ready {
                        theme::Intent::Success
                    } else {
                        theme::Intent::Warning
                    },
                );

                theme::status_chip(
                    ui,
                    if snap.decoder_ready {
                        "Decoder ready"
                    } else {
                        "Decoder unavailable"
                    },
                    if snap.decoder_ready {
                        theme::Intent::Success
                    } else {
                        theme::Intent::Warning
                    },
                );

                let (bridge_label, bridge_intent) = if snap.ml_bridge_connected {
                    if snap.ml_bridge_stalled {
                        ("ML bridge stalled", theme::Intent::Warning)
                    } else {
                        ("ML bridge connected", theme::Intent::Success)
                    }
                } else {
                    ("ML bridge disconnected", theme::Intent::Muted)
                };
                theme::status_chip(ui, bridge_label, bridge_intent);
            });

            ui.add_space(8.0);

            if snap.running {
                let stop_label = if state.config.service.runtime_mode == ServiceRuntimeMode::External {
                    "Request Shutdown"
                } else {
                    "Stop Service"
                };
                let stop_clicked =
                    theme::action_button(ui, stop_label, true, theme::ButtonTone::Secondary);
                if stop_clicked {
                    service_manager.stop();
                }

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    let reload_clicked = theme::action_button(
                        ui,
                        "Reload Model",
                        true,
                        theme::ButtonTone::Secondary,
                    );
                    if reload_clicked {
                        service_manager.reload_model();
                    }
                    let can_promote = snap.profile_ready;
                    let promote = theme::action_button(
                        ui,
                        "Promote Candidate",
                        can_promote,
                        theme::ButtonTone::Primary,
                    );
                    if promote {
                        service_manager.promote_candidate_model();
                    }
                });
                if !snap.profile_ready {
                    ui.label(
                        egui::RichText::new("Candidate promotion requires a calibrated active profile")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                }

                if state.config.ui.mode == UiMode::Advanced {
                    ui.add_space(6.0);
                    let has_profile = state.active_profile_id.is_some();
                    let can_train = !self.train_stage_task.is_pending() && has_profile;
                    let train_clicked = theme::action_button(
                        ui,
                        "Train + Stage Candidate",
                        can_train,
                        theme::ButtonTone::Primary,
                    );
                    if train_clicked {
                        self.start_train_stage_job(state);
                    }
                    if !has_profile {
                        ui.label(
                            egui::RichText::new("Training requires an active profile selection")
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                }
            } else {
                let start_label = if state.config.service.runtime_mode == ServiceRuntimeMode::External {
                    "Probe External Service"
                } else {
                    "Start Service"
                };
                let start_clicked =
                    theme::action_button(ui, start_label, true, theme::ButtonTone::Primary);
                if start_clicked {
                    service_manager.start(
                        runtime,
                        state.config.clone(),
                        Some(state.profile_store.clone()),
                        state.active_profile_id.clone(),
                    );
                }
                if state.config.service.runtime_mode == ServiceRuntimeMode::External {
                    theme::status_chip(
                        ui,
                        "External mode requires running neurohid-service",
                        theme::Intent::Warning,
                    );
                }
                if state.active_profile_id.is_none() {
                    theme::status_chip(
                        ui,
                        "No profile selected — discovery mode",
                        theme::Intent::Warning,
                    );
                }
            }

            if let Some(status) = &self.train_stage_status {
                let (intent, text) = match status {
                    TrainStageStatus::Running(msg) => (theme::Intent::Warning, msg.as_str()),
                    TrainStageStatus::Success(msg) => (theme::Intent::Success, msg.as_str()),
                    TrainStageStatus::Error(msg) => (theme::Intent::Danger, msg.as_str()),
                };
                ui.add_space(6.0);
                theme::status_chip(ui, text, intent);
            }

            if !self.train_stage_output.is_empty() {
                ui.add_space(4.0);
                ui.collapsing("Train + Stage Output", |ui| {
                    egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                        let _ = theme::textarea_readonly(
                            ui,
                            "dashboard_train_stage_output",
                            &mut self.train_stage_output,
                            6,
                            f32::INFINITY,
                        );
                    });
                });
            }

            if let Some(err) = service_manager.last_error() {
                ui.add_space(6.0);
                theme::status_chip(ui, err, theme::Intent::Danger);
            }
            });

        ui.add_space(12.0);

        theme::card_frame(ui).show(ui, |ui| {
            ui.collapsing("Diagnostics", |ui| {
            ui.horizontal_wrapped(|ui| {
                theme::status_chip(
                    ui,
                    &format!("Runtime {}", state.config.service.runtime_mode),
                    if snap.running {
                        theme::Intent::Info
                    } else {
                        theme::Intent::Muted
                    },
                );

                if routed_total > 0 {
                    theme::status_chip(
                        ui,
                        &format!("Routes {}", routed_total),
                        theme::Intent::Info,
                    );
                }

                if snap.latency_degraded {
                    theme::status_chip(ui, "Latency degraded", theme::Intent::Danger);
                } else if snap.running {
                    theme::status_chip(ui, "Latency nominal", theme::Intent::Success);
                }
            });
            ui.add_space(6.0);

            ui.horizontal_wrapped(|ui| {
                theme::status_chip(
                    ui,
                    &format!("Runtime {}", state.config.service.runtime_mode),
                    theme::Intent::Muted,
                );
                if let Some(version) = &snap.decoder_model_version {
                    theme::status_chip(ui, &format!("Model {}", version), theme::Intent::Info);
                }
                if let Some(model_kind) = &snap.fallback_model_kind {
                    theme::status_chip(
                        ui,
                        &format!("Path {}", model_kind),
                        theme::Intent::Muted,
                    );
                }
            });

            let capability_text = if snap.enabled_capabilities.is_empty() {
                "Capabilities none".to_string()
            } else {
                format!("Capabilities {}", snap.enabled_capabilities.join(", "))
            };
            theme::status_chip(ui, &capability_text, theme::Intent::Muted);
            if routed_total > 0 {
                theme::status_chip(
                    ui,
                    &format!(
                        "Routes EEG {} | Motion {} | Auxiliary {} | Unknown {}",
                        snap.routed_eeg_streams,
                        snap.routed_motion_streams,
                        snap.routed_auxiliary_streams,
                        snap.routed_unknown_streams
                    ),
                    theme::Intent::Info,
                );
            }
            if let Some(message) = &snap.limited_capabilities_message {
                let intent = if snap.runtime_mode_state == RuntimeModeState::Degraded {
                    theme::Intent::Danger
                } else {
                    theme::Intent::Warning
                };
                theme::status_chip(ui, message, intent);
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
                theme::status_chip(ui, &message, theme::Intent::Danger);
            }

            ui.add_space(8.0);
            ui.collapsing("ML Bridge & Trainer", |ui| {
                let mut learning_enabled = snap.learning_enabled;
                ui.horizontal(|ui| {
                    ui.label("Learning enabled");
                    if theme::toggle_switch(
                        ui,
                        "dashboard_learning_enabled",
                        &mut learning_enabled,
                    ) {
                        service_manager.set_learning_enabled(learning_enabled);
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    let bridge_text = if snap.ml_bridge_connected {
                        if snap.ml_bridge_stalled {
                            "Bridge stalled"
                        } else {
                            "Bridge connected"
                        }
                    } else {
                        "Bridge disconnected"
                    };

                    let bridge_intent = if snap.ml_bridge_connected {
                        if snap.ml_bridge_stalled {
                            theme::Intent::Warning
                        } else {
                            theme::Intent::Success
                        }
                    } else {
                        theme::Intent::Muted
                    };
                    theme::status_chip(ui, bridge_text, bridge_intent);

                    if let Some(trainer) = &self.trainer_snapshot {
                        let trainer_intent = if trainer.trainer_connected {
                            theme::Intent::Info
                        } else {
                            theme::Intent::Warning
                        };
                        theme::status_chip(
                            ui,
                            &format!("Trainer {}", trainer.trainer_state),
                            trainer_intent,
                        );
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    let reconnect_clicked = theme::action_button(
                        ui,
                        "Reconnect Bridge",
                        true,
                        theme::ButtonTone::Secondary,
                    );
                    if reconnect_clicked {
                        service_manager.ml_bridge_reconnect();
                    }
                    let apply_fallback_clicked = theme::action_button(
                        ui,
                        "Apply Fallback Policy",
                        true,
                        theme::ButtonTone::Secondary,
                    );
                    if apply_fallback_clicked {
                        service_manager
                            .set_fallback_policy(state.config.service.fallback_policy.clone());
                    }
                    let refresh_snapshot_clicked = theme::action_button(
                        ui,
                        "Refresh Trainer Snapshot",
                        true,
                        theme::ButtonTone::Ghost,
                    );
                    if refresh_snapshot_clicked {
                        self.trainer_snapshot = service_manager.trainer_snapshot();
                        self.last_trainer_snapshot_poll = Some(Instant::now());
                    }
                });

                if let Some(trainer) = &self.trainer_snapshot {
                    theme::status_chip(
                        ui,
                        &format!(
                            "Trainer {} | replay {} | step {}",
                            trainer.trainer_state, trainer.replay_size, trainer.training_step
                        ),
                        theme::Intent::Info,
                    );
                    if let Some(protocol) = trainer.protocol_version {
                        theme::status_chip(
                            ui,
                            &format!(
                                "Protocol v{} | connected {}",
                                protocol, trainer.trainer_connected
                            ),
                            theme::Intent::Muted,
                        );
                    }
                    if let Some(last_error) = &trainer.last_error {
                        theme::status_chip(ui, last_error, theme::Intent::Warning);
                    }
                } else {
                    theme::status_chip(
                        ui,
                        "Trainer snapshot unavailable",
                        theme::Intent::Muted,
                    );
                    theme::status_chip(
                        ui,
                        "Bridge disconnected / no trainer response",
                        theme::Intent::Muted,
                    );
                }

                self.show_trainer_observability(ui, snap);
            });

            if let Some((task, error)) = &snap.task_error {
                ui.add_space(8.0);
                theme::card_frame(ui)
                    .fill(egui::Color32::from_rgb(60, 20, 20))
                    .show(ui, |ui| {
                        theme::status_chip(
                            ui,
                            &format!("Service stopped: {} task failed", task),
                            theme::Intent::Danger,
                        );
                        theme::status_chip(ui, error, theme::Intent::Danger);
                        if let Some(hint) = task_error_hint(error) {
                            ui.add_space(4.0);
                            theme::status_chip(ui, hint, theme::Intent::Warning);
                        }
                    });
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
        self.train_stage_task.request(async move {
            tokio::task::spawn_blocking(move || {
                run_train_stage_candidate_job(profile_store, profile_id)
            })
            .await
            .map_err(|error| format!("Training job failed to join: {}", error))
        });
    }

    fn poll_train_stage_result(&mut self) {
        if let Some(result) = self.train_stage_task.take() {
            match result {
                Ok(result) => {
                    self.train_stage_status = Some(if result.success {
                        TrainStageStatus::Success(result.message)
                    } else {
                        TrainStageStatus::Error(result.message)
                    });
                    self.train_stage_output = result.output;
                }
                Err(error) => {
                self.train_stage_status = Some(TrainStageStatus::Error(
                    format!("Training job disconnected unexpectedly: {}", error),
                ));
                }
            }
        }
    }

    fn poll_trainer_snapshot(
        &mut self,
        service_manager: &mut ServiceManager,
        runtime_running: bool,
    ) {
        if !runtime_running {
            self.trainer_snapshot = None;
            self.last_trainer_snapshot_poll = None;
            return;
        }

        let now = Instant::now();
        if self
            .last_trainer_snapshot_poll
            .is_some_and(|last| now.duration_since(last) < Self::TRAINER_SNAPSHOT_POLL_INTERVAL)
        {
            return;
        }

        self.trainer_snapshot = service_manager.trainer_snapshot();
        self.last_trainer_snapshot_poll = Some(now);
    }

    fn sample_trainer_observability(&mut self, snap: &ServiceSnapshot) {
        if !snap.running {
            self.reset_observability();
            return;
        }

        let now = Instant::now();
        if self
            .last_observability_sample
            .is_some_and(|last| now.duration_since(last) < Self::OBSERVABILITY_SAMPLE_INTERVAL)
        {
            return;
        }
        self.last_observability_sample = Some(now);

        if let Some(replay_size) = snap.trainer_replay_size {
            self.replay_size_history.push(replay_size as f64);
        }
        if let Some(training_step) = snap.trainer_step {
            self.training_step_history.push(training_step as f64);
        }
        if let Some(policy_loss) = snap.trainer_policy_loss {
            self.policy_loss_history.push(policy_loss as f64);
        }
        if let Some(value_loss) = snap.trainer_value_loss {
            self.value_loss_history.push(value_loss as f64);
        }
        if let Some(entropy) = snap.trainer_entropy {
            self.entropy_history.push(entropy as f64);
        }
        self.candidate_promoted_history
            .push(snap.candidate_promotions_succeeded as f64);
        self.candidate_rejected_history
            .push(snap.candidate_promotions_rejected as f64);

        if let Some(outcome) = snap.candidate_last_outcome.clone() {
            let changed = self
                .last_candidate_outcome
                .as_ref()
                .is_none_or(|prev| prev != &outcome);
            if changed {
                self.recent_candidate_outcomes.push_back(outcome.clone());
                while self.recent_candidate_outcomes.len() > Self::CANDIDATE_OUTCOME_HISTORY_LIMIT {
                    let _ = self.recent_candidate_outcomes.pop_front();
                }
                self.last_candidate_outcome = Some(outcome);
            }
        }
    }

    fn reset_observability(&mut self) {
        self.last_observability_sample = None;
        self.replay_size_history.clear();
        self.training_step_history.clear();
        self.policy_loss_history.clear();
        self.value_loss_history.clear();
        self.entropy_history.clear();
        self.candidate_promoted_history.clear();
        self.candidate_rejected_history.clear();
        self.recent_candidate_outcomes.clear();
        self.last_candidate_outcome = None;
    }

    fn show_trainer_observability(&self, ui: &mut egui::Ui, snap: &ServiceSnapshot) {
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label(
                egui::RichText::new("Trainer Observability")
                    .small()
                    .strong(),
            );
            ui.add_space(4.0);

            ui.columns(2, |cols| {
                Self::draw_sparkline(
                    &mut cols[0],
                    "Replay Size",
                    &self.replay_size_history,
                    egui::Color32::from_rgb(80, 170, 255),
                    0,
                    "",
                );
                Self::draw_sparkline(
                    &mut cols[1],
                    "Training Step",
                    &self.training_step_history,
                    egui::Color32::from_rgb(120, 220, 150),
                    0,
                    "",
                );
            });

            ui.columns(3, |cols| {
                Self::draw_sparkline(
                    &mut cols[0],
                    "Policy Loss",
                    &self.policy_loss_history,
                    egui::Color32::from_rgb(255, 180, 70),
                    4,
                    "",
                );
                Self::draw_sparkline(
                    &mut cols[1],
                    "Value Loss",
                    &self.value_loss_history,
                    egui::Color32::from_rgb(255, 120, 120),
                    4,
                    "",
                );
                Self::draw_sparkline(
                    &mut cols[2],
                    "Entropy",
                    &self.entropy_history,
                    egui::Color32::from_rgb(190, 140, 255),
                    4,
                    "",
                );
            });

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(format!("Promoted: {}", snap.candidate_promotions_succeeded));
                ui.separator();
                ui.label(format!("Rejected: {}", snap.candidate_promotions_rejected));
            });

            let total = snap
                .candidate_promotions_succeeded
                .saturating_add(snap.candidate_promotions_rejected);
            if total > 0 {
                let success_ratio = snap.candidate_promotions_succeeded as f32 / total as f32;
                ui.label(format!(
                    "Candidate promotion success {:.0}% ({}/{})",
                    success_ratio * 100.0,
                    snap.candidate_promotions_succeeded,
                    total
                ));
                let _ = theme::progress_bar(ui, success_ratio, ui.available_width());
            }

            ui.columns(2, |cols| {
                Self::draw_sparkline(
                    &mut cols[0],
                    "Promoted Trend",
                    &self.candidate_promoted_history,
                    egui::Color32::from_rgb(80, 200, 120),
                    0,
                    "",
                );
                Self::draw_sparkline(
                    &mut cols[1],
                    "Rejected Trend",
                    &self.candidate_rejected_history,
                    egui::Color32::from_rgb(220, 90, 90),
                    0,
                    "",
                );
            });

            if let Some(last) = &snap.candidate_last_outcome {
                let intent = if last.to_ascii_lowercase().contains("rejected") {
                    theme::Intent::Warning
                } else {
                    theme::Intent::Success
                };
                theme::status_chip(ui, last, intent);
            }

            if !self.recent_candidate_outcomes.is_empty() {
                ui.collapsing("Recent Candidate Outcomes", |ui| {
                    for outcome in self.recent_candidate_outcomes.iter().rev() {
                        ui.label(
                            egui::RichText::new(format!("- {}", outcome))
                                .small()
                                .color(egui::Color32::GRAY),
                        );
                    }
                });
            }
        });
    }

    fn draw_sparkline(
        ui: &mut egui::Ui,
        label: &str,
        history: &MetricHistory,
        color: egui::Color32,
        precision: usize,
        suffix: &str,
    ) {
        ui.label(
            egui::RichText::new(format!(
                "{}: {}",
                label,
                Self::format_metric(history.latest(), precision, suffix)
            ))
            .small(),
        );

        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(17, 22, 30));
        painter.rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 56, 73)),
            egui::StrokeKind::Inside,
        );

        for division in 1..4 {
            let y = rect.top() + rect.height() * (division as f32 / 4.0);
            painter.hline(
                rect.x_range(),
                y,
                egui::Stroke::new(0.8, egui::Color32::from_rgb(33, 42, 56)),
            );
        }

        if history.len() < 2 {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "collecting samples...",
                egui::FontId::proportional(11.0),
                egui::Color32::GRAY,
            );
            return;
        }

        let Some((min, max)) = history.min_max() else {
            return;
        };
        let span = (max - min).max(1e-6);
        let denom = (history.len().saturating_sub(1)) as f32;
        let points: Vec<egui::Pos2> = history
            .values
            .iter()
            .enumerate()
            .map(|(idx, value)| {
                let x = if denom <= 0.0 {
                    rect.left()
                } else {
                    rect.left() + rect.width() * (idx as f32 / denom)
                };
                let normalized = ((*value - min) / span) as f32;
                let y = rect.bottom() - normalized * rect.height();
                egui::pos2(x, y)
            })
            .collect();

        if let (Some(first), Some(last)) = (points.first().copied(), points.last().copied()) {
            let mut area_points = Vec::with_capacity(points.len() + 2);
            area_points.push(egui::pos2(first.x, rect.bottom()));
            area_points.extend(points.iter().copied());
            area_points.push(egui::pos2(last.x, rect.bottom()));
            painter.add(egui::Shape::convex_polygon(
                area_points,
                color.gamma_multiply(0.16),
                egui::Stroke::NONE,
            ));
        }

        painter.add(egui::Shape::line(
            points.clone(),
            egui::Stroke::new(1.7, color),
        ));

        if let Some(last_point) = points.last() {
            painter.circle_filled(*last_point, 2.4, color);
        }

        if response.hovered() {
            response.on_hover_text(format!(
                "latest {} | min {} | max {}",
                Self::format_metric(history.latest(), precision, suffix),
                Self::format_metric(Some(min), precision, suffix),
                Self::format_metric(Some(max), precision, suffix)
            ));
        }
    }

    fn format_metric(value: Option<f64>, precision: usize, suffix: &str) -> String {
        match value {
            Some(v) => format!("{:.*}{}", precision, v, suffix),
            None => "n/a".to_string(),
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

