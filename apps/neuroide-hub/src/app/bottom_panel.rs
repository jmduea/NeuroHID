use super::HubApp;
use crate::screens::Screen;
use crate::theme;
use crate::workbench::BottomTab;
use eframe::egui;
use neurohid_types::config::UiMode;
use std::collections::BTreeMap;

impl HubApp {
    pub(crate) fn show_bottom_panel(&mut self, ctx: &egui::Context) {
        if self.state.config.ui.mode != UiMode::Advanced || !self.workbench.bottom_panel.visible {
            return;
        }

        let mut hide_panel = false;
        let response = egui::TopBottomPanel::bottom("workbench_bottom_panel")
            .resizable(true)
            .default_height(self.workbench.bottom_panel.height.clamp(
                theme::WORKBENCH_BOTTOM_MIN_HEIGHT,
                theme::WORKBENCH_BOTTOM_MAX_HEIGHT,
            ))
            .min_height(theme::WORKBENCH_BOTTOM_MIN_HEIGHT)
            .max_height(theme::WORKBENCH_BOTTOM_MAX_HEIGHT)
            .frame(
                egui::Frame::new()
                    .fill(theme::workbench_surface_fill_ctx(ctx))
                    .inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for tab in advanced_bottom_panel_tabs() {
                        let selected = self.workbench.bottom_panel.active_tab == tab;
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new(tab.label()).small())
                                    .frame(false)
                                    .min_size(egui::vec2(76.0, 24.0))
                                    .selected(selected),
                            )
                            .clicked()
                        {
                            self.workbench.open_bottom_tab(tab);
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::action_button(ui, "Hide", true, theme::ButtonTone::Ghost) {
                            hide_panel = true;
                        }
                    });
                });

                ui.separator();

                match self.workbench.bottom_panel.active_tab {
                    BottomTab::Console => {
                        self.stream_console.show_embedded(
                            ui,
                            &self.data_bus,
                            &self.state.service_snapshot,
                        );
                    }
                    BottomTab::Logs => self.show_logs_bottom_tab(ui),
                    BottomTab::Runtime => self.show_runtime_bottom_tab(ui, ctx),
                    BottomTab::Problems => self.show_problems_bottom_tab(ui),
                }
            });

        self.workbench.bottom_panel.height = response.response.rect.height().clamp(
            theme::WORKBENCH_BOTTOM_MIN_HEIGHT,
            theme::WORKBENCH_BOTTOM_MAX_HEIGHT,
        );
        if hide_panel {
            self.workbench.bottom_panel.visible = false;
        }
    }
    pub(crate) fn show_logs_bottom_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let clear_clicked =
                theme::action_button(ui, "Clear", true, theme::ButtonTone::Secondary);
            if clear_clicked {
                egui_logger::clear_logs();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui_logger::logger_ui().show(ui);
            });
    }
    pub(crate) fn show_runtime_bottom_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let running = self.state.service_snapshot.running;
        let ipc_connected = self.state.service_snapshot.ipc_connected;
        let ipc_simulated = self.state.service_snapshot.ipc_simulated;
        let bridge_connected = self.state.service_snapshot.ml_bridge_connected;
        let bridge_stalled = self.state.service_snapshot.ml_bridge_stalled;
        let signal_quality = self.state.service_snapshot.signal_quality;
        let uptime_secs = self.state.service_snapshot.uptime_secs;
        let task_error = self.state.service_snapshot.task_error.clone();
        let pipeline_integrity_degraded = self.state.service_snapshot.pipeline_integrity_degraded;
        let integrity_issue_count = self.state.service_snapshot.integrity_issue_count;
        let stage_health_summary = self.state.service_snapshot.stage_health_summary.clone();
        let routed_total = self.state.service_snapshot.routed_eeg_streams
            + self.state.service_snapshot.routed_motion_streams
            + self.state.service_snapshot.routed_auxiliary_streams
            + self.state.service_snapshot.routed_unknown_streams;
        let uptime_mins = uptime_secs / 60;
        let uptime_rem_secs = uptime_secs % 60;

        ui.horizontal_wrapped(|ui| {
            theme::status_chip(
                ui,
                if running {
                    "Service running"
                } else {
                    "Service stopped"
                },
                if running {
                    theme::Intent::Success
                } else {
                    theme::Intent::Muted
                },
            );
            theme::status_chip(
                ui,
                if ipc_connected {
                    if ipc_simulated {
                        "IPC simulated"
                    } else {
                        "IPC connected"
                    }
                } else {
                    "IPC disconnected"
                },
                if ipc_connected {
                    theme::Intent::Info
                } else {
                    theme::Intent::Warning
                },
            );
            theme::status_chip(
                ui,
                if bridge_connected {
                    if bridge_stalled {
                        "Bridge stalled"
                    } else {
                        "Bridge connected"
                    }
                } else {
                    "Bridge disconnected"
                },
                if bridge_connected && !bridge_stalled {
                    theme::Intent::Success
                } else if bridge_stalled {
                    theme::Intent::Warning
                } else {
                    theme::Intent::Muted
                },
            );
            theme::status_chip(ui, &format!("Routes {}", routed_total), theme::Intent::Info);
            theme::status_chip(
                ui,
                &format!("Uptime {}:{:02}", uptime_mins, uptime_rem_secs),
                theme::Intent::Muted,
            );
            theme::status_chip(
                ui,
                &format!("Signal {:.0}%", signal_quality * 100.0),
                if signal_quality >= 0.7 {
                    theme::Intent::Success
                } else if signal_quality >= 0.5 {
                    theme::Intent::Warning
                } else {
                    theme::Intent::Danger
                },
            );
            theme::status_chip(
                ui,
                &format!("Integrity issues {}", integrity_issue_count),
                if integrity_issue_count == 0 {
                    theme::Intent::Muted
                } else if pipeline_integrity_degraded {
                    theme::Intent::Danger
                } else {
                    theme::Intent::Warning
                },
            );
            theme::status_chip(
                ui,
                if pipeline_integrity_degraded {
                    "Pipeline integrity degraded"
                } else {
                    "Pipeline integrity stable"
                },
                if pipeline_integrity_degraded {
                    theme::Intent::Danger
                } else {
                    theme::Intent::Success
                },
            );

            if let Some((task, _)) = task_error {
                theme::status_chip(ui, &format!("{task} task error"), theme::Intent::Danger);
            }
        });

        if let Some(summary) = stage_health_summary {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Stage health").small().weak());
            ui.label(egui::RichText::new(summary).small().monospace());
        }

        ui.add_space(6.0);

        ui.horizontal_wrapped(|ui| {
            if theme::action_button(
                ui,
                "Reconnect Bridge",
                running,
                theme::ButtonTone::Secondary,
            ) {
                self.service_manager.ml_bridge_reconnect();
            }

            if theme::action_button(ui, "Refresh Snapshot", true, theme::ButtonTone::Ghost) {
                self.state.service_snapshot = self.service_manager.snapshot();
                let now = ctx.input(|input| input.time);
                self.workbench
                    .record_runtime_events(&self.state.service_snapshot, now);
            }

            if theme::action_button(
                ui,
                "Apply Fallback Policy",
                running,
                theme::ButtonTone::Secondary,
            ) {
                self.service_manager
                    .set_fallback_policy(self.state.config.service.fallback_policy.clone());
            }
        });

        ui.separator();
        ui.label(egui::RichText::new("Runtime timeline").small().weak());

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut count = 0usize;
            for event in self.workbench.runtime_events() {
                if count >= 40 {
                    break;
                }
                count += 1;
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!("[{:>7.1}s]", event.timestamp_secs))
                            .small()
                            .weak()
                            .monospace(),
                    );
                    ui.label(egui::RichText::new(&event.message).small());
                });
            }
            if count == 0 {
                theme::status_chip(ui, "No runtime transitions yet", theme::Intent::Muted);
            }
        });
    }
    pub(crate) fn show_problems_bottom_tab(&mut self, ui: &mut egui::Ui) {
        let problems = self.collect_problem_rows();
        if problems.is_empty() {
            theme::status_chip(ui, "No active problems detected", theme::Intent::Success);
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for problem in problems {
                theme::card_frame(ui).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        theme::status_chip(ui, &problem.message, problem.intent);
                        if let Some(screen) = problem.screen_target
                            && theme::action_button(
                                ui,
                                &format!("Open {}", screen.label()),
                                true,
                                theme::ButtonTone::Ghost,
                            )
                        {
                            self.current_screen = screen;
                            self.workbench.sync_lane_from_screen(
                                &self.state.config.ui.mode,
                                self.current_screen,
                            );
                            self.persist_last_screen();
                            if let Some(tab) = problem.tab_target {
                                self.workbench.open_bottom_tab(tab);
                            }
                        }
                    });
                });
            }
        });
    }
    pub(crate) fn collect_problem_rows(&self) -> Vec<ProblemRow> {
        let mut rows = Vec::new();
        let snap = &self.state.service_snapshot;
        let routed_total = snap.routed_eeg_streams
            + snap.routed_motion_streams
            + snap.routed_auxiliary_streams
            + snap.routed_unknown_streams;

        if let Some(error) = &self.state.init_error {
            rows.push(ProblemRow {
                message: format!("Initialization error: {}", error),
                intent: theme::Intent::Danger,
                screen_target: Some(Screen::Settings),
                tab_target: Some(BottomTab::Problems),
            });
        }

        if let Some((task, error)) = &snap.task_error {
            rows.push(ProblemRow {
                message: format!("{task} task failed: {error}"),
                intent: theme::Intent::Danger,
                screen_target: Some(Screen::Dashboard),
                tab_target: Some(BottomTab::Runtime),
            });
        }

        if let Some(error) = &snap.trainer_last_error {
            rows.push(ProblemRow {
                message: format!("Trainer error: {}", error),
                intent: theme::Intent::Warning,
                screen_target: Some(Screen::PythonLab),
                tab_target: Some(BottomTab::Problems),
            });
        }

        if let Some(error) = self.dashboard.active_train_stage_error() {
            rows.push(ProblemRow {
                message: format!("Train + Stage candidate failed: {}", error),
                intent: theme::Intent::Warning,
                screen_target: Some(Screen::PythonLab),
                tab_target: Some(BottomTab::Problems),
            });
        }

        if snap.running && !snap.ml_bridge_connected {
            rows.push(ProblemRow {
                message: "ML bridge disconnected while runtime is running".to_string(),
                intent: theme::Intent::Warning,
                screen_target: Some(Screen::Dashboard),
                tab_target: Some(BottomTab::Runtime),
            });
        }

        if snap.ml_bridge_stalled {
            rows.push(ProblemRow {
                message: "ML bridge stalled".to_string(),
                intent: theme::Intent::Warning,
                screen_target: Some(Screen::Dashboard),
                tab_target: Some(BottomTab::Runtime),
            });
        }

        if snap.running && !snap.ipc_connected {
            rows.push(ProblemRow {
                message: "Runtime IPC disconnected".to_string(),
                intent: theme::Intent::Warning,
                screen_target: Some(Screen::Dashboard),
                tab_target: Some(BottomTab::Runtime),
            });
        }

        if snap.running && snap.latency_degraded {
            rows.push(ProblemRow {
                message: snap
                    .latency_alert_message
                    .clone()
                    .unwrap_or_else(|| "Runtime latency degraded".to_string()),
                intent: theme::Intent::Warning,
                screen_target: Some(Screen::Visualization),
                tab_target: Some(BottomTab::Runtime),
            });
        }

        if snap.running && snap.integrity_issue_count > 0 {
            rows.push(ProblemRow {
                message: if snap.pipeline_integrity_degraded {
                    snap.stage_health_summary.clone().unwrap_or_else(|| {
                        format!(
                            "Pipeline integrity degraded ({} issues)",
                            snap.integrity_issue_count
                        )
                    })
                } else {
                    snap.stage_health_summary.clone().unwrap_or_else(|| {
                        format!(
                            "Pipeline integrity warning ({} issues)",
                            snap.integrity_issue_count
                        )
                    })
                },
                intent: if snap.pipeline_integrity_degraded {
                    theme::Intent::Danger
                } else {
                    theme::Intent::Warning
                },
                screen_target: Some(Screen::Visualization),
                tab_target: Some(BottomTab::Runtime),
            });
        }

        if snap.running && !snap.profile_ready {
            rows.push(ProblemRow {
                message: "Active profile is not calibrated".to_string(),
                intent: theme::Intent::Warning,
                screen_target: Some(Screen::Profiles),
                tab_target: Some(BottomTab::Problems),
            });
        }

        if snap.calibration_mode && !snap.running {
            rows.push(ProblemRow {
                message: "Calibration mode is active while runtime is stopped".to_string(),
                intent: theme::Intent::Warning,
                screen_target: Some(Screen::Calibration),
                tab_target: Some(BottomTab::Runtime),
            });
        }

        if snap.calibration_mode && snap.output_enabled {
            rows.push(ProblemRow {
                message: "Calibration mode active while output remains enabled".to_string(),
                intent: theme::Intent::Danger,
                screen_target: Some(Screen::Calibration),
                tab_target: Some(BottomTab::Runtime),
            });
        }

        if let Some(device_health_row) = build_device_health_problem_row(
            snap,
            routed_total,
            &self.state.config.ui.device_health_problems,
        ) {
            rows.push(device_health_row);
        }

        normalize_problem_rows(rows)
    }
}

pub(crate) struct ProblemRow {
    pub(crate) message: String,
    pub(crate) intent: theme::Intent,
    pub(crate) screen_target: Option<Screen>,
    pub(crate) tab_target: Option<BottomTab>,
}

pub(crate) fn problem_severity_rank(intent: theme::Intent) -> u8 {
    match intent {
        theme::Intent::Danger => 4,
        theme::Intent::Warning => 3,
        theme::Intent::Info => 2,
        theme::Intent::Muted => 1,
        theme::Intent::Success => 0,
    }
}

pub(crate) fn normalize_problem_rows(rows: Vec<ProblemRow>) -> Vec<ProblemRow> {
    let mut deduped_by_message: BTreeMap<String, ProblemRow> = BTreeMap::new();

    for row in rows {
        let key = row.message.to_ascii_lowercase();
        match deduped_by_message.entry(key) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                let _ = entry.insert(row);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let existing = entry.get();
                let row_rank = problem_severity_rank(row.intent);
                let existing_rank = problem_severity_rank(existing.intent);
                let prefer_row = row_rank > existing_rank
                    || (row_rank == existing_rank
                        && existing.screen_target.is_none()
                        && row.screen_target.is_some());
                if prefer_row {
                    let _ = entry.insert(row);
                }
            }
        }
    }

    let mut normalized: Vec<ProblemRow> = deduped_by_message.into_values().collect();
    normalized.sort_by(|a, b| {
        problem_severity_rank(b.intent)
            .cmp(&problem_severity_rank(a.intent))
            .then_with(|| {
                a.message
                    .to_ascii_lowercase()
                    .cmp(&b.message.to_ascii_lowercase())
            })
    });
    normalized
}

pub(crate) fn build_device_health_problem_row(
    snap: &crate::state::ServiceSnapshot,
    routed_total: u64,
    thresholds: &neurohid_types::config::DeviceHealthProblemConfig,
) -> Option<ProblemRow> {
    if !snap.running {
        return None;
    }

    let battery_low_pct = thresholds.battery_low_threshold_pct.clamp(1, 100);
    let battery_critical_pct = thresholds
        .battery_critical_threshold_pct
        .clamp(1, battery_low_pct.saturating_sub(1).max(1));
    let quality_warning_threshold = thresholds.quality_warning_threshold.clamp(0.0, 1.0);
    let quality_critical_threshold = thresholds
        .quality_critical_threshold
        .clamp(0.0, quality_warning_threshold);

    let connected_streams: Vec<_> = snap
        .discovered_streams
        .iter()
        .filter(|stream| stream.connected)
        .collect();

    let mut issues = Vec::new();
    let mut issue_intent = theme::Intent::Warning;
    let mut runtime_focused = false;

    let mut push_issue = |message: String, intent: theme::Intent| {
        if problem_severity_rank(intent) > problem_severity_rank(issue_intent) {
            issue_intent = intent;
        }
        issues.push(message);
    };

    if snap.discovered_streams.is_empty() {
        push_issue("No streams discovered".to_string(), theme::Intent::Warning);
    } else if connected_streams.is_empty() {
        push_issue("No connected streams".to_string(), theme::Intent::Warning);
    }

    if !connected_streams.is_empty() && routed_total == 0 {
        push_issue(
            "Connected streams have no active routes".to_string(),
            theme::Intent::Warning,
        );
        runtime_focused = true;
    }

    if let Some((stream_name, battery)) = connected_streams
        .iter()
        .filter_map(|stream| {
            stream
                .battery_percent
                .map(|battery| (stream.name.as_str(), battery))
        })
        .min_by_key(|(_, battery)| *battery)
        && battery <= battery_low_pct
    {
        if battery <= battery_critical_pct {
            push_issue(
                format!("Critical battery on {stream_name} ({battery}%)"),
                theme::Intent::Danger,
            );
        } else {
            push_issue(
                format!("Low battery on {stream_name} ({battery}%)"),
                theme::Intent::Warning,
            );
        }
    }

    if let Some((stream_name, quality)) = connected_streams
        .iter()
        .filter_map(|stream| {
            let channel_quality = stream.channel_quality.as_ref()?;
            if channel_quality.is_empty() {
                return None;
            }
            let mean = channel_quality.iter().copied().sum::<f32>() / channel_quality.len() as f32;
            (mean < quality_warning_threshold).then_some((stream.name.as_str(), mean))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        if quality < quality_critical_threshold {
            push_issue(
                format!(
                    "Critical channel quality on {stream_name} ({:.0}%)",
                    quality * 100.0
                ),
                theme::Intent::Danger,
            );
        } else {
            push_issue(
                format!(
                    "Poor channel quality on {stream_name} ({:.0}%)",
                    quality * 100.0
                ),
                theme::Intent::Warning,
            );
        }
        runtime_focused = true;
    }

    if issues.is_empty() {
        return None;
    }

    let headline = if issue_intent == theme::Intent::Danger {
        "Device health critical"
    } else {
        "Device health warning"
    };

    let message = match issues.as_slice() {
        [single] => format!("{headline}: {single}"),
        [first, second] => format!("{headline}: {first}; {second}"),
        [first, second, rest @ ..] => {
            format!("{headline}: {first}; {second} (+{} more)", rest.len())
        }
        [] => unreachable!("checked above"),
    };

    Some(ProblemRow {
        message,
        intent: issue_intent,
        screen_target: Some(Screen::Devices),
        tab_target: Some(if runtime_focused {
            BottomTab::Runtime
        } else {
            BottomTab::Problems
        }),
    })
}

pub(crate) fn advanced_status_bar_tabs() -> [BottomTab; 4] {
    [
        BottomTab::Problems,
        BottomTab::Runtime,
        BottomTab::Logs,
        BottomTab::Console,
    ]
}

pub(crate) fn advanced_bottom_panel_tabs() -> [BottomTab; 4] {
    [
        BottomTab::Problems,
        BottomTab::Runtime,
        BottomTab::Logs,
        BottomTab::Console,
    ]
}
