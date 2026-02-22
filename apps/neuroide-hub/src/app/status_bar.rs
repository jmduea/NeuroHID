use super::HubApp;
use crate::app::bottom_panel::advanced_status_bar_tabs;
use crate::screens::Screen;
use crate::state::ServiceSnapshot;
use crate::theme;
use crate::workbench::{ActivityLane, BottomTab, WorkbenchState};
use eframe::egui;
use neurohid_types::config::UiMode;
use neurohid_types::control::RuntimeModeState;
use std::collections::BTreeMap;

impl HubApp {
    /// Persistent device/stream strip (status bar): visible from every screen.
    /// Single source of truth: ServiceSnapshot (ControlSnapshot). Shows device
    /// count (connected/total) and signal quality so users have one place for
    /// at-a-glance status without opening the Devices screen.
    pub(crate) fn show_status_bar(&mut self, ctx: &egui::Context) {
        const FOOTER_CONTROL_WIDTH_ADVANCED: f32 = 278.0;
        const FOOTER_CONTROL_WIDTH_STANDARD: f32 = 140.0;
        const FOOTER_ITEM_WIDTH: f32 = 64.0;
        const FOOTER_ITEM_WIDTH_STANDARD: f32 = 62.0;

        let advanced_mode = self.state.config.ui.mode == UiMode::Advanced;
        let footer_control_width = if advanced_mode {
            FOOTER_CONTROL_WIDTH_ADVANCED
        } else {
            FOOTER_CONTROL_WIDTH_STANDARD
        };

        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(theme::WORKBENCH_STATUS_BAR_HEIGHT)
            .show(ctx, |ui| {
                let snap = &self.state.service_snapshot;
                let full_rect = ui.available_rect_before_wrap();
                let left_max_x = (full_rect.right()
                    - footer_control_width
                    - theme::WORKBENCH_STATUS_DIVIDER_GAP)
                    .max(full_rect.left());

                let left_rect = egui::Rect::from_min_max(
                    full_rect.min,
                    egui::pos2(left_max_x, full_rect.bottom()),
                );
                let right_rect = egui::Rect::from_min_max(
                    egui::pos2(
                        (left_max_x + theme::WORKBENCH_STATUS_DIVIDER_GAP).min(full_rect.right()),
                        full_rect.top(),
                    ),
                    full_rect.max,
                );

                let mut left_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(left_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                left_ui.horizontal(|ui| {
                    let service_intent = if snap.running {
                        theme::Intent::Success
                    } else if snap.task_error.is_some() {
                        theme::Intent::Danger
                    } else {
                        theme::Intent::Muted
                    };
                    theme::status_chip(
                        ui,
                        if snap.running {
                            "Service running"
                        } else {
                            "Service stopped"
                        },
                        service_intent,
                    );

                    let (ipc_label, ipc_intent) = if snap.ipc_connected {
                        if snap.ipc_simulated {
                            ("IPC simulated", theme::Intent::Warning)
                        } else {
                            ("IPC connected", theme::Intent::Info)
                        }
                    } else if snap.running {
                        ("IPC disconnected", theme::Intent::Warning)
                    } else {
                        ("IPC disconnected", theme::Intent::Muted)
                    };
                    theme::status_chip(ui, ipc_label, ipc_intent);

                    let routed_total = snap.routed_eeg_streams
                        + snap.routed_motion_streams
                        + snap.routed_auxiliary_streams
                        + snap.routed_unknown_streams;
                    if routed_total > 0 {
                        theme::status_chip(
                            ui,
                            &format!("Routes {}", routed_total),
                            theme::Intent::Info,
                        );
                    }

                    if let Some((task, _)) = &snap.task_error {
                        theme::status_chip(
                            ui,
                            &format!("{} task error", task),
                            theme::Intent::Danger,
                        );
                    }

                    if self.visualization_detached_fallback_warning {
                        theme::status_chip(
                            ui,
                            "Detached visualization unavailable; using embedded view",
                            theme::Intent::Warning,
                        );
                    }

                    // Persistent strip: device count and signal health always visible (one place)
                    let connected_streams = snap
                        .discovered_streams
                        .iter()
                        .filter(|stream| stream.connected)
                        .count();
                    let total_streams = snap.discovered_streams.len();
                    theme::status_chip(
                        ui,
                        &format!("Devices {}/{}", connected_streams, total_streams),
                        if connected_streams > 0 {
                            theme::Intent::Info
                        } else {
                            theme::Intent::Muted
                        },
                    );
                    let signal_intent = if snap.signal_quality >= 0.7 {
                        theme::Intent::Success
                    } else if snap.signal_quality >= 0.5 {
                        theme::Intent::Warning
                    } else {
                        theme::Intent::Danger
                    };
                    theme::status_chip(
                        ui,
                        &format!("Signal {:.0}%", snap.signal_quality * 100.0),
                        signal_intent,
                    );

                    if snap.running {
                        let mode_label = match snap.runtime_mode_state {
                            RuntimeModeState::Full => "Mode: full",
                            RuntimeModeState::Fallback => "Mode: fallback",
                            RuntimeModeState::Degraded => "Mode: degraded",
                        };
                        let mode_intent = match snap.runtime_mode_state {
                            RuntimeModeState::Full => theme::Intent::Success,
                            RuntimeModeState::Fallback => theme::Intent::Warning,
                            RuntimeModeState::Degraded => theme::Intent::Danger,
                        };
                        theme::status_chip(ui, mode_label, mode_intent);

                        let mins = snap.uptime_secs / 60;
                        let secs = snap.uptime_secs % 60;
                        theme::status_chip(
                            ui,
                            &format!("Uptime {}:{:02}", mins, secs),
                            theme::Intent::Muted,
                        );

                        if snap.calibration_mode {
                            theme::status_chip(ui, "Calibrating", theme::Intent::Warning);
                        }
                    }
                });

                if right_rect.width() > 0.0 {
                    let stroke_color = theme::workbench_divider_color(ui);
                    let divider_x = left_max_x + theme::WORKBENCH_STATUS_DIVIDER_GAP * 0.5;
                    ui.painter().line_segment(
                        [
                            egui::pos2(divider_x, full_rect.top() + 4.0),
                            egui::pos2(divider_x, full_rect.bottom() - 4.0),
                        ],
                        egui::Stroke::new(1.0, stroke_color),
                    );
                }

                let mut right_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(right_rect)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                right_ui.spacing_mut().item_spacing.x = 2.0;

                let footer_item =
                    |ui: &mut egui::Ui, label: &str, selected: bool, width: f32| -> bool {
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(width, theme::WORKBENCH_STATUS_ITEM_HEIGHT),
                            egui::Sense::click(),
                        );

                        let visuals = ui.style().visuals.clone();
                        let bg_fill = if selected {
                            visuals.selection.bg_fill.gamma_multiply(0.45)
                        } else if response.hovered() {
                            visuals.widgets.hovered.bg_fill.gamma_multiply(0.35)
                        } else {
                            egui::Color32::TRANSPARENT
                        };

                        if bg_fill != egui::Color32::TRANSPARENT {
                            ui.painter()
                                .rect_filled(rect, egui::CornerRadius::same(4), bg_fill);
                        }

                        if selected {
                            let underline_y = rect.bottom() - 1.0;
                            ui.painter().line_segment(
                                [
                                    egui::pos2(rect.left() + 6.0, underline_y),
                                    egui::pos2(rect.right() - 6.0, underline_y),
                                ],
                                egui::Stroke::new(1.5, visuals.selection.stroke.color),
                            );
                        }

                        let text_color = if selected {
                            visuals.strong_text_color()
                        } else if response.hovered() {
                            visuals.text_color()
                        } else {
                            visuals.weak_text_color()
                        };

                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            label,
                            egui::TextStyle::Small.resolve(ui.style()),
                            text_color,
                        );

                        response.clicked()
                    };

                right_ui.add_space(4.0);
                let show_visualization_toggle = self.current_screen == Screen::Visualization
                    || self.state.config.ui.visualization_detached;
                if show_visualization_toggle {
                    let detach_label = if self.state.config.ui.visualization_detached {
                        "Attach Viz"
                    } else {
                        "Detach Viz"
                    };
                    if theme::action_button(
                        &mut right_ui,
                        detach_label,
                        true,
                        theme::ButtonTone::Ghost,
                    ) {
                        self.toggle_visualization_detached(ctx);
                    }
                    right_ui.add_space(4.0);
                }
                if advanced_mode {
                    for tab in advanced_status_bar_tabs() {
                        let selected = self.workbench.bottom_panel.visible
                            && self.workbench.bottom_panel.active_tab == tab;
                        if footer_item(&mut right_ui, tab.label(), selected, FOOTER_ITEM_WIDTH) {
                            self.workbench.toggle_bottom_tab(tab);
                        }
                    }
                } else {
                    if footer_item(
                        &mut right_ui,
                        "Console",
                        self.stream_console.visible,
                        FOOTER_ITEM_WIDTH_STANDARD,
                    ) {
                        self.stream_console.toggle();
                    }
                    if footer_item(
                        &mut right_ui,
                        "Logs",
                        self.show_log_window,
                        FOOTER_ITEM_WIDTH_STANDARD,
                    ) {
                        self.show_log_window = !self.show_log_window;
                    }
                }

                ui.allocate_rect(full_rect, egui::Sense::hover());
            });
    }
}
