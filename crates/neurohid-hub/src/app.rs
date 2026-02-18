//! # Hub Application
//!
//! The main `eframe::App` implementation that ties together the sidebar,
//! status bar, and screen dispatch.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use armas::components::{CollapsibleMode, Sidebar, SidebarResponse, SidebarState, SidebarVariant};
use armas::prelude::ArmasContextExt;
use eframe::egui;
use neurohid_types::config::UiMode;
use neurohid_types::control::RuntimeModeState;

use crate::data_bus::DataBus;
use crate::screens::Screen;
use crate::screens::calibration::CalibrationScreen;
use crate::screens::dashboard::DashboardScreen;
use crate::screens::devices::DevicesScreen;
use crate::screens::jupyter_ide::JupyterIdeScreen;
use crate::screens::profiles::ProfilesScreen;
use crate::screens::python_lab::PythonLabScreen;
use crate::screens::settings::SettingsScreen;
use crate::screens::visualization::VisualizationScreen;
use crate::service_manager::ServiceManager;
use crate::state::HubState;
use crate::stream_console::StreamConsole;
use crate::theme;
use crate::workbench::{ActivityLane, BottomTab, WorkbenchState};

/// The main hub application.
pub struct HubApp {
    runtime: tokio::runtime::Runtime,
    current_screen: Screen,
    state: HubState,
    service_manager: ServiceManager,
    last_service_running: Option<bool>,
    last_latency_degraded: Option<bool>,
    last_runtime_mode_running: Option<bool>,
    last_runtime_mode_state: Option<RuntimeModeState>,
    data_bus: DataBus,
    stream_console: StreamConsole,
    sidebar_state: SidebarState,
    workbench: WorkbenchState,
    show_log_window: bool,
    // Screen instances
    dashboard: DashboardScreen,
    visualization: VisualizationScreen,
    devices: DevicesScreen,
    profiles: ProfilesScreen,
    calibration: CalibrationScreen,
    jupyter_ide: JupyterIdeScreen,
    python_lab: PythonLabScreen,
    settings: SettingsScreen,
}

impl HubApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, runtime: tokio::runtime::Runtime) -> Self {
        // Initialize storage (blocking on the runtime since we're in the main thread)
        let (state, init_error) = match runtime.block_on(Self::init_state()) {
            Ok(state) => (state, None),
            Err(e) => {
                let error_msg = e.to_string();
                tracing::error!("Failed to initialize: {}", error_msg);

                // Create a minimal fallback state without panicking.
                let (fallback, fallback_error) = runtime.block_on(Self::init_fallback_state());
                let combined_error = match fallback_error {
                    Some(fallback_error) => {
                        format!(
                            "{} | fallback storage degraded: {}",
                            error_msg, fallback_error
                        )
                    }
                    None => error_msg,
                };

                (fallback, Some(combined_error))
            }
        };

        let visualization = VisualizationScreen::from_ui_config(&state.config.ui);

        let mut hub = Self {
            runtime,
            current_screen: Screen::Dashboard,
            state,
            service_manager: ServiceManager::new(),
            last_service_running: None,
            last_latency_degraded: None,
            last_runtime_mode_running: None,
            last_runtime_mode_state: None,
            data_bus: DataBus::new(),
            stream_console: StreamConsole::new(),
            sidebar_state: SidebarState::new(true),
            workbench: WorkbenchState::default(),
            show_log_window: false,
            dashboard: DashboardScreen::new(),
            visualization,
            devices: DevicesScreen::new(),
            profiles: ProfilesScreen::new(),
            calibration: CalibrationScreen::new(),
            jupyter_ide: JupyterIdeScreen::new(),
            python_lab: PythonLabScreen::new(),
            settings: SettingsScreen::new(),
        };

        if let Some(err) = init_error {
            hub.state.init_error = Some(err);
        }

        hub.service_manager.configure(&hub.state.config);

        if hub.state.config.service.auto_start {
            let profile_store = Some(hub.state.profile_store.clone());
            let profile_id = hub.state.active_profile_id.clone();
            hub.service_manager.start(
                &hub.runtime,
                hub.state.config.clone(),
                profile_store,
                profile_id,
            );
        }

        hub
    }

    async fn init_state() -> anyhow::Result<HubState> {
        let (profile_store, config_store) = neurohid_storage::initialize()
            .await
            .map_err(|e| anyhow::anyhow!("Storage init failed: {}", e))?;

        let mut config = config_store
            .load()
            .await
            .map_err(|e| anyhow::anyhow!("Config load failed: {}", e))?;

        if !config.service.auto_start {
            config.service.auto_start = true;
            if let Err(error) = config_store.save(&config).await {
                tracing::warn!(
                    error = %error,
                    "Failed to persist migrated service auto-start default"
                );
            }
        }

        let profiles = profile_store.list_profiles().await.unwrap_or_default();

        Ok(HubState::new(profile_store, config_store, config, profiles))
    }

    async fn init_fallback_state() -> (HubState, Option<String>) {
        let fallback_root = std::env::temp_dir().join("neurohid-fallback");
        let paths = match neurohid_storage::DataPaths::new(Some(fallback_root.clone())) {
            Ok(paths) => paths,
            Err(e) => {
                let current_dir_root: PathBuf = PathBuf::from(".neurohid-fallback");
                let error_msg = format!(
                    "failed to create fallback paths at {}: {}",
                    fallback_root.display(),
                    e
                );
                tracing::error!("{}", error_msg);

                match neurohid_storage::DataPaths::new(Some(current_dir_root.clone())) {
                    Ok(paths) => {
                        let profile_store = neurohid_storage::ProfileStore::new(
                            paths.clone(),
                            neurohid_storage::SecureStorage::default(),
                        );
                        let config_store = neurohid_storage::ConfigStore::new(paths);
                        let state = HubState::new(
                            profile_store,
                            config_store,
                            neurohid_types::config::SystemConfig::default(),
                            vec![],
                        );
                        return (
                            state,
                            Some(format!(
                                "{}; using relative fallback storage at {}",
                                error_msg,
                                current_dir_root.display()
                            )),
                        );
                    }
                    Err(second_error) => {
                        tracing::error!(
                            "failed to create relative fallback paths at {}: {}",
                            current_dir_root.display(),
                            second_error
                        );
                        let paths = neurohid_storage::DataPaths::new(Some(std::env::temp_dir()))
                            .unwrap_or_else(|_| {
                                unreachable!("temp-dir fallback path should be valid")
                            });
                        let profile_store = neurohid_storage::ProfileStore::new(
                            paths.clone(),
                            neurohid_storage::SecureStorage::default(),
                        );
                        let config_store = neurohid_storage::ConfigStore::new(paths);
                        let state = HubState::new(
                            profile_store,
                            config_store,
                            neurohid_types::config::SystemConfig::default(),
                            vec![],
                        );
                        return (
                            state,
                            Some(format!(
                                "{}; secondary fallback failed: {}",
                                error_msg, second_error
                            )),
                        );
                    }
                }
            }
        };

        if let Err(e) = paths.ensure_directories().await {
            tracing::warn!(
                "fallback storage directory initialization failed at {}: {}",
                paths.root().display(),
                e
            );
            let profile_store = neurohid_storage::ProfileStore::new(
                paths.clone(),
                neurohid_storage::SecureStorage::default(),
            );
            let config_store = neurohid_storage::ConfigStore::new(paths);
            let state = HubState::new(
                profile_store,
                config_store,
                neurohid_types::config::SystemConfig::default(),
                vec![],
            );
            return (state, Some(format!("directory init failed: {}", e)));
        }

        let profile_store = neurohid_storage::ProfileStore::new(
            paths.clone(),
            neurohid_storage::SecureStorage::default(),
        );
        let config_store = neurohid_storage::ConfigStore::new(paths);

        (
            HubState::new(
                profile_store,
                config_store,
                neurohid_types::config::SystemConfig::default(),
                vec![],
            ),
            None,
        )
    }

    fn show_sidebar(&mut self, ctx: &egui::Context) {
        let panel_width = self.sidebar_state.width().clamp(52.0, 280.0);
        let screens = self.workbench.visible_screens(&self.state.config.ui.mode);
        let mut response = SidebarShellResponse::default();
        let sidebar_theme = ctx.armas_theme();

        egui::SidePanel::left("sidebar")
            .exact_width(panel_width)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(sidebar_theme.sidebar())
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                response = render_sidebar_shell(
                    ui,
                    &mut self.sidebar_state,
                    screens,
                    self.current_screen,
                    self.workbench.lane,
                    self.workbench.sidebar_focus_screen,
                );
            });

        apply_sidebar_shell_response(
            &self.state.config.ui.mode,
            response,
            &mut self.workbench,
            &mut self.current_screen,
        );
    }

    fn show_status_bar(&mut self, ctx: &egui::Context) {
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

    fn show_bottom_panel(&mut self, ctx: &egui::Context) {
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

    fn show_logs_bottom_tab(&mut self, ui: &mut egui::Ui) {
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

    fn show_runtime_bottom_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let running = self.state.service_snapshot.running;
        let ipc_connected = self.state.service_snapshot.ipc_connected;
        let ipc_simulated = self.state.service_snapshot.ipc_simulated;
        let bridge_connected = self.state.service_snapshot.ml_bridge_connected;
        let bridge_stalled = self.state.service_snapshot.ml_bridge_stalled;
        let signal_quality = self.state.service_snapshot.signal_quality;
        let uptime_secs = self.state.service_snapshot.uptime_secs;
        let task_error = self.state.service_snapshot.task_error.clone();
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

            if let Some((task, _)) = task_error {
                theme::status_chip(ui, &format!("{task} task error"), theme::Intent::Danger);
            }
        });

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

    fn show_problems_bottom_tab(&mut self, ui: &mut egui::Ui) {
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
                        if let Some(screen) = problem.screen_target {
                            if theme::action_button(
                                ui,
                                &format!("Open {}", screen.label()),
                                true,
                                theme::ButtonTone::Ghost,
                            ) {
                                self.current_screen = screen;
                                self.workbench.sync_lane_from_screen(
                                    &self.state.config.ui.mode,
                                    self.current_screen,
                                );
                                if let Some(tab) = problem.tab_target {
                                    self.workbench.open_bottom_tab(tab);
                                }
                            }
                        }
                    });
                });
            }
        });
    }

    fn collect_problem_rows(&self) -> Vec<ProblemRow> {
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

    fn focus_sidebar_navigation(&mut self) {
        self.sidebar_state.set_open(true);
        let screens = self.workbench.visible_screens(&self.state.config.ui.mode);
        if screens.is_empty() {
            return;
        }
        if !screens.contains(&self.current_screen) {
            self.current_screen = screens[0];
            self.workbench
                .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);
        }
        self.workbench.sidebar_focus_screen = Some(self.current_screen);
    }

    fn step_sidebar_navigation(&mut self, step: i32) {
        let screens = self.workbench.visible_screens(&self.state.config.ui.mode);
        if screens.is_empty() {
            return;
        }
        self.sidebar_state.set_open(true);

        let current_index = screens
            .iter()
            .position(|screen| *screen == self.current_screen)
            .unwrap_or(0) as i32;
        let next_index = (current_index + step).rem_euclid(screens.len() as i32) as usize;
        self.current_screen = screens[next_index];
        self.workbench
            .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);
        self.workbench.sidebar_focus_screen = Some(self.current_screen);
    }

    fn handle_workbench_shortcuts(&mut self, ctx: &egui::Context) {
        if self.state.config.ui.mode != UiMode::Advanced {
            return;
        }

        let (
            open_palette,
            lane_shortcut,
            toggle_bottom_panel,
            cycle_right,
            cycle_left,
            toggle_sidebar,
            focus_sidebar,
            sidebar_next,
            sidebar_prev,
            escape_palette,
        ) = ctx.input(|input| {
            let modifiers = input.modifiers;
            let open_palette =
                modifiers.command && modifiers.shift && input.key_pressed(egui::Key::P);
            let lane_shortcut = if modifiers.command && modifiers.shift {
                if input.key_pressed(egui::Key::O) {
                    Some(ActivityLane::Ops)
                } else if input.key_pressed(egui::Key::A) {
                    Some(ActivityLane::Analysis)
                } else if input.key_pressed(egui::Key::L) {
                    Some(ActivityLane::Labs)
                } else if input.key_pressed(egui::Key::C) {
                    Some(ActivityLane::Config)
                } else {
                    None
                }
            } else {
                None
            };
            let toggle_bottom_panel = modifiers.command && input.key_pressed(egui::Key::J);
            let cycle_right = modifiers.alt && input.key_pressed(egui::Key::ArrowRight);
            let cycle_left = modifiers.alt && input.key_pressed(egui::Key::ArrowLeft);
            let toggle_sidebar = modifiers.command && input.key_pressed(egui::Key::B);
            let focus_sidebar =
                modifiers.command && modifiers.shift && input.key_pressed(egui::Key::S);
            let sidebar_next =
                modifiers.command && modifiers.shift && input.key_pressed(egui::Key::ArrowDown);
            let sidebar_prev =
                modifiers.command && modifiers.shift && input.key_pressed(egui::Key::ArrowUp);
            let escape_palette = input.key_pressed(egui::Key::Escape);
            (
                open_palette,
                lane_shortcut,
                toggle_bottom_panel,
                cycle_right,
                cycle_left,
                toggle_sidebar,
                focus_sidebar,
                sidebar_next,
                sidebar_prev,
                escape_palette,
            )
        });

        if open_palette {
            self.workbench.command_palette_open = true;
            self.workbench.command_query.clear();
            self.workbench.command_palette_focus_index = 0;
        }
        if let Some(lane) = lane_shortcut {
            self.workbench
                .set_lane(&self.state.config.ui.mode, lane, &mut self.current_screen);
            self.workbench.sidebar_focus_screen = Some(self.current_screen);
        }
        if toggle_bottom_panel {
            self.workbench.bottom_panel.visible = !self.workbench.bottom_panel.visible;
        }
        if cycle_right {
            self.workbench.cycle_bottom_tab(1);
        }
        if cycle_left {
            self.workbench.cycle_bottom_tab(-1);
        }
        if toggle_sidebar {
            self.sidebar_state.set_open(!self.sidebar_state.is_open());
        }
        if focus_sidebar {
            self.focus_sidebar_navigation();
        }
        if sidebar_next {
            self.step_sidebar_navigation(1);
        }
        if sidebar_prev {
            self.step_sidebar_navigation(-1);
        }
        if escape_palette {
            self.workbench.command_palette_open = false;
        }
    }

    fn show_command_palette(&mut self, ctx: &egui::Context) {
        if self.state.config.ui.mode != UiMode::Advanced || !self.workbench.command_palette_open {
            return;
        }

        let mut selected_action: Option<CommandPaletteAction> = None;
        let mut close_palette = false;

        egui::Window::new("Command Palette")
            .id(egui::Id::new("workbench_command_palette"))
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 18.0))
            .fixed_size(egui::vec2(460.0, 320.0))
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.workbench.command_query)
                        .hint_text("Type a command (Ctrl+Shift+P, ↑/↓ to navigate)"),
                );
                response.request_focus();
                if response.changed() {
                    self.workbench.command_palette_focus_index = 0;
                }

                ui.separator();

                let query = self.workbench.command_query.to_ascii_lowercase();
                let actions = command_palette_items(self.state.service_snapshot.running);
                let mut filtered: Vec<_> = actions
                    .into_iter()
                    .filter(|item| {
                        query.is_empty()
                            || item.label.to_ascii_lowercase().contains(&query)
                            || item.keywords.to_ascii_lowercase().contains(&query)
                    })
                    .collect();
                filtered.truncate(16);

                if filtered.is_empty() {
                    self.workbench.command_palette_focus_index = 0;
                } else {
                    self.workbench.command_palette_focus_index = self
                        .workbench
                        .command_palette_focus_index
                        .min(filtered.len() - 1);
                }

                let (up_pressed, down_pressed, enter_pressed, escape_pressed) = ui.input(|input| {
                    (
                        input.key_pressed(egui::Key::ArrowUp),
                        input.key_pressed(egui::Key::ArrowDown),
                        input.key_pressed(egui::Key::Enter),
                        input.key_pressed(egui::Key::Escape),
                    )
                });

                if !filtered.is_empty() {
                    if up_pressed {
                        self.workbench.command_palette_focus_index =
                            if self.workbench.command_palette_focus_index == 0 {
                                filtered.len() - 1
                            } else {
                                self.workbench.command_palette_focus_index - 1
                            };
                    } else if down_pressed {
                        self.workbench.command_palette_focus_index =
                            (self.workbench.command_palette_focus_index + 1) % filtered.len();
                    }
                }

                egui::ScrollArea::vertical()
                    .max_height(250.0)
                    .show(ui, |ui| {
                        for (index, item) in filtered.iter().enumerate() {
                            let selected = index == self.workbench.command_palette_focus_index;
                            let label = if selected {
                                format!("▸ {}", item.label)
                            } else {
                                item.label.to_string()
                            };
                            let response = ui.add(
                                egui::Button::new(label)
                                    .frame(!selected)
                                    .selected(selected)
                                    .min_size(egui::vec2(ui.available_width(), 24.0)),
                            );
                            if response.hovered() {
                                self.workbench.command_palette_focus_index = index;
                            }
                            if response.clicked() {
                                selected_action = Some(item.action);
                            }
                        }
                    });

                if enter_pressed
                    && selected_action.is_none()
                    && let Some(item) = filtered.get(self.workbench.command_palette_focus_index)
                {
                    selected_action = Some(item.action);
                }
                if escape_pressed {
                    close_palette = true;
                }
            });

        if close_palette {
            self.workbench.command_palette_open = false;
            self.workbench.command_query.clear();
            self.workbench.command_palette_focus_index = 0;
            return;
        }

        if let Some(action) = selected_action {
            self.execute_command_palette_action(action);
            self.workbench.command_palette_open = false;
            self.workbench.command_query.clear();
            self.workbench.command_palette_focus_index = 0;
        }
    }

    fn execute_command_palette_action(&mut self, action: CommandPaletteAction) {
        match action {
            CommandPaletteAction::OpenScreen(screen) => {
                self.current_screen = screen;
                self.workbench
                    .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);
            }
            CommandPaletteAction::ToggleBottomPanel => {
                self.workbench.bottom_panel.visible = !self.workbench.bottom_panel.visible;
            }
            CommandPaletteAction::OpenBottomTab(tab) => {
                self.workbench.open_bottom_tab(tab);
            }
            CommandPaletteAction::ReconnectBridge => {
                self.service_manager.ml_bridge_reconnect();
                self.workbench.open_bottom_tab(BottomTab::Runtime);
            }
            CommandPaletteAction::RefreshRuntimeSnapshot => {
                self.state.service_snapshot = self.service_manager.snapshot();
                self.workbench.open_bottom_tab(BottomTab::Runtime);
            }
            CommandPaletteAction::ApplyFallbackPolicy => {
                self.service_manager
                    .set_fallback_policy(self.state.config.service.fallback_policy.clone());
                self.workbench.open_bottom_tab(BottomTab::Runtime);
            }
            CommandPaletteAction::StartService => {
                self.service_manager.start(
                    &self.runtime,
                    self.state.config.clone(),
                    Some(self.state.profile_store.clone()),
                    self.state.active_profile_id.clone(),
                );
            }
            CommandPaletteAction::StopService => {
                self.service_manager.stop();
            }
            CommandPaletteAction::PrepareEnvironment => {
                self.jupyter_ide
                    .command_prepare_environment(&self.state.config.ui);
                self.current_screen = Screen::JupyterIde;
                self.workbench
                    .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);
            }
            CommandPaletteAction::StartJupyter => {
                self.jupyter_ide
                    .command_start_jupyter(&self.state.config.ui);
                self.current_screen = Screen::JupyterIde;
                self.workbench
                    .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);
            }
            CommandPaletteAction::StopJupyter => {
                self.jupyter_ide.command_stop_jupyter();
                self.current_screen = Screen::JupyterIde;
                self.workbench
                    .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);
            }
            CommandPaletteAction::OpenJupyterBrowser => {
                if let Err(error) = self
                    .jupyter_ide
                    .command_open_in_browser(&self.state.config.ui)
                {
                    tracing::warn!(
                        "Failed to open Jupyter browser from command palette: {}",
                        error
                    );
                }
            }
        }
    }

    fn maybe_notify_latency_transition(&mut self) {
        let snapshot = &self.state.service_snapshot;
        let was_running = self.last_service_running.replace(snapshot.running);
        let was_degraded = self
            .last_latency_degraded
            .replace(snapshot.latency_degraded);

        if !self.state.config.service.notifications_enabled {
            return;
        }

        let (Some(was_running), Some(was_degraded)) = (was_running, was_degraded) else {
            return;
        };

        if !was_running || !snapshot.running || was_degraded == snapshot.latency_degraded {
            return;
        }

        if snapshot.latency_degraded {
            let message = snapshot
                .latency_alert_message
                .clone()
                .unwrap_or_else(|| "Runtime latency exceeded configured thresholds.".to_string());
            self.send_desktop_notification("NeuroHID latency warning", &message);
        } else {
            self.send_desktop_notification(
                "NeuroHID latency recovered",
                "Runtime latency returned within configured thresholds.",
            );
        }
    }

    fn maybe_notify_runtime_mode_transition(&mut self) {
        let snapshot = &self.state.service_snapshot;
        let was_running = self.last_runtime_mode_running.replace(snapshot.running);
        let previous_mode = self
            .last_runtime_mode_state
            .replace(snapshot.runtime_mode_state);

        if !self.state.config.service.notifications_enabled {
            return;
        }

        let (Some(was_running), Some(previous_mode)) = (was_running, previous_mode) else {
            return;
        };
        if !was_running || !snapshot.running || previous_mode == snapshot.runtime_mode_state {
            return;
        }

        let (title, fallback_body) = match snapshot.runtime_mode_state {
            RuntimeModeState::Full => (
                "NeuroHID runtime mode: full",
                "Runtime recovered to full capability mode.",
            ),
            RuntimeModeState::Fallback => (
                "NeuroHID runtime mode: fallback",
                "Runtime entered fallback mode; capabilities may be limited.",
            ),
            RuntimeModeState::Degraded => (
                "NeuroHID runtime mode: degraded",
                "Runtime entered degraded mode; HID output may be limited or disabled.",
            ),
        };

        let body = snapshot
            .limited_capabilities_message
            .as_deref()
            .unwrap_or(fallback_body);
        self.send_desktop_notification(title, body);
    }

    fn send_desktop_notification(&self, title: &str, body: &str) {
        if let Err(error) = desktop_notify(title, body) {
            tracing::debug!(
                title = title,
                error = %error,
                "Desktop notification dispatch failed"
            );
        }
    }
}

fn render_sidebar_shell(
    ui: &mut egui::Ui,
    sidebar_state: &mut SidebarState,
    screens: &[Screen],
    current_screen: Screen,
    current_lane: ActivityLane,
    keyboard_focus_screen: Option<Screen>,
) -> SidebarShellResponse {
    let shell_rect = ui.available_rect_before_wrap();
    ui.set_min_height(shell_rect.height());
    let sidebar_open = sidebar_state.is_open();

    let footer_height = if sidebar_open { 44.0 } else { 36.0 };
    let footer_top = (shell_rect.bottom() - footer_height).max(shell_rect.top());
    let body_rect =
        egui::Rect::from_min_max(shell_rect.min, egui::pos2(shell_rect.right(), footer_top));
    let footer_rect =
        egui::Rect::from_min_max(egui::pos2(shell_rect.left(), footer_top), shell_rect.max);

    let mut body_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(body_rect)
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
    );
    body_ui.spacing_mut().item_spacing = if sidebar_open {
        egui::vec2(8.0, 7.0)
    } else {
        egui::vec2(6.0, 5.0)
    };

    for lane in [
        ActivityLane::Ops,
        ActivityLane::Analysis,
        ActivityLane::Labs,
    ] {
        sidebar_test_marker(&mut body_ui, lane.label());
    }
    sidebar_test_marker(&mut body_ui, "Settings");
    if sidebar_open {
        sidebar_test_marker(&mut body_ui, "Lanes");
        sidebar_test_marker(&mut body_ui, "Platform");
    }

    for &screen in screens {
        sidebar_test_marker(&mut body_ui, screen.label());
    }

    let platform_response = render_platform_sidebar(
        &mut body_ui,
        sidebar_state,
        screens,
        current_screen,
        current_lane,
        keyboard_focus_screen,
    );
    let mut lane_selection = None;
    let mut clicked_nav_id = None;
    if let Some(clicked_id) = platform_response.clicked {
        lane_selection = lane_selection_from_clicked_id(&clicked_id);
        if lane_selection.is_none() {
            clicked_nav_id = Some(clicked_id);
        }
    }
    if sidebar_open && let Some(focus_screen) = keyboard_focus_screen {
        body_ui.add_space(4.0);
        theme::status_chip(
            &mut body_ui,
            &format!("Keyboard focus {}", focus_screen.label()),
            theme::Intent::Muted,
        );
    }

    ui.painter().hline(
        shell_rect.x_range(),
        footer_top,
        egui::Stroke::new(1.0, theme::workbench_divider_color(ui).gamma_multiply(0.9)),
    );

    let mut footer_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(footer_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let open_settings = render_settings_anchor(&mut footer_ui, current_lane, sidebar_state);

    ui.allocate_rect(shell_rect, egui::Sense::hover());

    SidebarShellResponse {
        clicked_nav_id,
        lane_selection,
        open_settings,
    }
}

fn render_platform_sidebar(
    ui: &mut egui::Ui,
    sidebar_state: &mut SidebarState,
    screens: &[Screen],
    current_screen: Screen,
    current_lane: ActivityLane,
    keyboard_focus_screen: Option<Screen>,
) -> SidebarResponse {
    let sidebar_open = sidebar_state.is_open();
    let mut hover_labels: Vec<Option<String>> = Vec::new();
    let response = ui
        .push_id("platform_sidebar", |ui| {
            Sidebar::new()
                .state(sidebar_state)
                .variant(SidebarVariant::Sidebar)
                .collapsible(CollapsibleMode::Icon)
                .show(ui, |sidebar| {
                    if sidebar_open {
                        sidebar.group_label("Lanes");
                        hover_labels.push(None);
                    }

                    for lane in [
                        ActivityLane::Ops,
                        ActivityLane::Analysis,
                        ActivityLane::Labs,
                    ] {
                        sidebar
                            .item(lane.glyph(), lane.label())
                            .active(current_lane == lane);
                        hover_labels.push(Some(lane.label().to_string()));
                    }

                    if sidebar_open {
                        sidebar.group_label("Platform");
                        hover_labels.push(None);
                    }

                    for &screen in screens {
                        let glyph = if keyboard_focus_screen == Some(screen) {
                            ">"
                        } else {
                            screen_glyph(screen)
                        };
                        sidebar
                            .item(glyph, screen.label())
                            .active(current_screen == screen);
                        hover_labels.push(Some(screen.label().to_string()));
                    }
                })
        })
        .inner;

    if !response.is_expanded
        && let Some(label) = response
            .hovered
            .and_then(|index| hover_labels.get(index))
            .and_then(|entry| entry.as_deref())
    {
        let _ = egui::Tooltip::always_open(
            ui.ctx().clone(),
            ui.layer_id(),
            egui::Id::new("platform_sidebar_icon_tip"),
            egui::PopupAnchor::Pointer,
        )
        .gap(12.0)
        .show(|ui| {
            ui.label(label);
        });
    }

    response
}

fn lane_selection_from_clicked_id(clicked_id: &str) -> Option<ActivityLane> {
    for lane in [
        ActivityLane::Ops,
        ActivityLane::Analysis,
        ActivityLane::Labs,
    ] {
        if clicked_id == format!("item_0_{}", lane.label()) {
            return Some(lane);
        }
    }
    None
}

fn render_settings_anchor(
    ui: &mut egui::Ui,
    current_lane: ActivityLane,
    sidebar_state: &SidebarState,
) -> bool {
    let sidebar_open = sidebar_state.is_open();
    let label = if sidebar_open { "ST Settings" } else { "ST" };
    ui.add_space(4.0);
    let min_width = if sidebar_open {
        (ui.available_width() - 4.0).max(32.0)
    } else {
        30.0
    };
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .strong()
                .text_style(egui::TextStyle::Body),
        )
        .frame(false)
        .min_size(egui::vec2(min_width, 28.0))
        .selected(current_lane == ActivityLane::Config),
    )
    .on_hover_text("Settings")
    .clicked()
}

#[derive(Debug, Clone, Default)]
struct SidebarShellResponse {
    clicked_nav_id: Option<String>,
    lane_selection: Option<ActivityLane>,
    open_settings: bool,
}

fn apply_sidebar_shell_response(
    mode: &UiMode,
    response: SidebarShellResponse,
    workbench: &mut WorkbenchState,
    current_screen: &mut Screen,
) {
    if let Some(lane) = response.lane_selection {
        workbench.set_lane(mode, lane, current_screen);
        workbench.sidebar_focus_screen = None;
    }

    if response.open_settings {
        workbench.set_lane(mode, ActivityLane::Config, current_screen);
        *current_screen = Screen::Settings;
        workbench.sidebar_focus_screen = None;
    }

    if let Some(clicked_id) = response.clicked_nav_id {
        for &screen in workbench.visible_screens(mode) {
            let nav_id = format!("item_0_{}", screen.label());
            if clicked_id == nav_id {
                *current_screen = screen;
                workbench.sidebar_focus_screen = None;
                workbench.sync_lane_from_screen(mode, *current_screen);
                break;
            }
        }
    }
}

#[cfg(test)]
fn sidebar_test_marker(ui: &mut egui::Ui, label: &str) {
    ui.label(label);
}

#[cfg(not(test))]
fn sidebar_test_marker(_ui: &mut egui::Ui, _label: &str) {}

impl eframe::App for HubApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.plugin_or_default::<egui_async::EguiAsyncPlugin>();

        theme::apply_ui_preferences(
            ctx,
            self.state.config.ui.theme_mode.clone(),
            self.state.config.ui.font_scale,
        );

        if !Screen::all_for_mode(&self.state.config.ui.mode).contains(&self.current_screen) {
            self.current_screen = Screen::Dashboard;
        }
        self.workbench
            .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);

        // Keep manager mode/endpoint in sync with persisted runtime settings.
        self.service_manager.configure(&self.state.config);

        // Poll service state (non-blocking)
        self.state.service_snapshot = self.service_manager.snapshot();
        let now = ctx.input(|input| input.time);
        self.workbench
            .record_runtime_events(&self.state.service_snapshot, now);
        self.maybe_notify_latency_transition();
        self.maybe_notify_runtime_mode_transition();

        self.handle_workbench_shortcuts(ctx);

        // Connect/disconnect the data bus based on service state
        self.service_manager.sync_data_bus(&mut self.data_bus);

        // Poll data bus — drain broadcast channels into ring buffers
        self.data_bus.poll();

        // Update stream console with new data
        self.stream_console
            .update(&self.data_bus, &self.state.service_snapshot);

        // Panel ordering matters in egui: top/bottom panels should be shown
        // before side panels so side content does not get clipped at the bottom.
        self.show_status_bar(ctx);
        self.show_sidebar(ctx);
        self.show_bottom_panel(ctx);

        if self.state.config.ui.mode != UiMode::Advanced {
            // Legacy console/log presentation for Standard mode.
            self.stream_console
                .show(ctx, &self.data_bus, &self.state.service_snapshot);

            if self.show_log_window {
                egui::Window::new("Runtime Logs")
                    .open(&mut self.show_log_window)
                    .default_size(egui::vec2(760.0, 320.0))
                    .vscroll(true)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            let clear_clicked = theme::action_button(
                                ui,
                                "Clear",
                                true,
                                theme::ButtonTone::Secondary,
                            );
                            if clear_clicked {
                                egui_logger::clear_logs();
                            }
                        });
                        ui.separator();
                        egui_logger::logger_ui().show(ui);
                    });
            }
        }

        // When calibration is active, the CalibrationPanel renders its own
        // CentralPanel directly into the remaining space (after sidebar/status bar).
        // We skip our own CentralPanel to avoid the double-panel conflict.
        if self.current_screen == Screen::Calibration && self.calibration.is_panel_active() {
            self.calibration.show_active_panel(
                &mut self.state,
                &mut self.service_manager,
                &self.runtime,
                ctx,
            );
            ctx.request_repaint_after(Duration::from_millis(16));
            return;
        }

        // Central panel — dispatch to the active screen
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin::symmetric(8, 8)),
            )
            .show(ctx, |ui| {
                // Show init error if any
                if let Some(err) = &self.state.init_error {
                    theme::status_chip(ui, &format!("Init error: {}", err), theme::Intent::Danger);
                    ui.separator();
                }

                match self.current_screen {
                    Screen::Dashboard => {
                        self.dashboard.show(
                            ui,
                            &self.state,
                            &mut self.service_manager,
                            &self.runtime,
                        );
                        if self.dashboard.take_open_runtime_panel_request()
                            && self.state.config.ui.mode == UiMode::Advanced
                        {
                            self.workbench.open_bottom_tab(BottomTab::Runtime);
                        }
                        if self.dashboard.take_open_problems_panel_request()
                            && self.state.config.ui.mode == UiMode::Advanced
                        {
                            self.workbench.open_bottom_tab(BottomTab::Problems);
                        }
                    }
                    Screen::Visualization => {
                        let snapshot = self.state.service_snapshot.clone();
                        self.visualization.show(
                            ui,
                            &self.data_bus,
                            &snapshot,
                            &mut self.state,
                            &self.runtime,
                        );
                    }
                    Screen::Devices => {
                        self.devices
                            .show(ui, &self.state, &mut self.service_manager);
                    }
                    Screen::Profiles => {
                        self.profiles.show(
                            ui,
                            &mut self.state,
                            &self.runtime,
                            &self.service_manager,
                        );
                    }
                    Screen::Calibration => {
                        self.calibration
                            .show_entry(ui, &mut self.state, &mut self.service_manager);
                    }
                    Screen::JupyterIde => {
                        self.jupyter_ide.show(ui, &self.state.config.ui);
                    }
                    Screen::PythonLab => {
                        self.python_lab.show(
                            ui,
                            &self.state.config.ui.jupyter_command,
                            &self.data_bus,
                            &self.state.service_snapshot,
                        );
                    }
                    Screen::Settings => {
                        self.settings.show(
                            ui,
                            &mut self.state,
                            &self.service_manager,
                            &self.runtime,
                        );
                    }
                }
            });

        self.show_command_palette(ctx);

        let frame_interval = if self.current_screen == Screen::Visualization {
            if self.state.service_snapshot.running {
                let fps = u64::from(self.state.config.ui.visualization_target_fps.clamp(5, 60));
                Duration::from_millis((1000 / fps).max(1))
            } else {
                Duration::from_millis(100)
            }
        } else if self.state.service_snapshot.running {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(200)
        };

        ctx.request_repaint_after(frame_interval);
    }
}

fn screen_glyph(screen: Screen) -> &'static str {
    match screen {
        Screen::Dashboard => "DB",
        Screen::Visualization => "VZ",
        Screen::Devices => "DV",
        Screen::Profiles => "PF",
        Screen::Calibration => "CL",
        Screen::JupyterIde => "JP",
        Screen::PythonLab => "PY",
        Screen::Settings => "ST",
    }
}

impl Drop for HubApp {
    fn drop(&mut self) {
        // Stop the background service before the tokio runtime is dropped.
        // Without this, the LSL spawn_blocking thread keeps running (its
        // `streaming` flag is never set to false), which prevents
        // Runtime::drop from completing — causing the process to hang on
        // window close.
        self.service_manager.stop();
    }
}

fn desktop_notify(title: &str, body: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        desktop_notify_windows(title, body)
    }
    #[cfg(unix)]
    {
        return desktop_notify_unix(title, body);
    }
    #[cfg(not(any(target_os = "windows", unix)))]
    {
        let _ = (title, body);
        Ok(())
    }
}

#[cfg(unix)]
fn desktop_notify_unix(title: &str, body: &str) -> std::io::Result<()> {
    let status = std::process::Command::new("notify-send")
        .arg(title)
        .arg(body)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "notify-send exited with status {}",
            status
        )))
    }
}

#[cfg(target_os = "windows")]
fn desktop_notify_windows(title: &str, body: &str) -> std::io::Result<()> {
    let script = "$ErrorActionPreference='Stop';\
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null;\
[Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] > $null;\
$title=[System.Security.SecurityElement]::Escape($env:NEUROHID_NOTIFY_TITLE);\
$body=[System.Security.SecurityElement]::Escape($env:NEUROHID_NOTIFY_BODY);\
$xml=\"<toast><visual><binding template='ToastGeneric'><text>$title</text><text>$body</text></binding></visual></toast>\";\
$doc=New-Object Windows.Data.Xml.Dom.XmlDocument;\
$doc.LoadXml($xml);\
$toast=[Windows.UI.Notifications.ToastNotification]::new($doc);\
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('NeuroHID').Show($toast);";

    let status = std::process::Command::new("powershell")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .env("NEUROHID_NOTIFY_TITLE", title)
        .env("NEUROHID_NOTIFY_BODY", body)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "powershell exited with status {}",
            status
        )))
    }
}

struct ProblemRow {
    message: String,
    intent: theme::Intent,
    screen_target: Option<Screen>,
    tab_target: Option<BottomTab>,
}

fn problem_severity_rank(intent: theme::Intent) -> u8 {
    match intent {
        theme::Intent::Danger => 4,
        theme::Intent::Warning => 3,
        theme::Intent::Info => 2,
        theme::Intent::Muted => 1,
        theme::Intent::Success => 0,
    }
}

fn normalize_problem_rows(rows: Vec<ProblemRow>) -> Vec<ProblemRow> {
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

fn build_device_health_problem_row(
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

#[derive(Debug, Clone, Copy)]
enum CommandPaletteAction {
    OpenScreen(Screen),
    ToggleBottomPanel,
    OpenBottomTab(BottomTab),
    ReconnectBridge,
    RefreshRuntimeSnapshot,
    ApplyFallbackPolicy,
    StartService,
    StopService,
    PrepareEnvironment,
    StartJupyter,
    StopJupyter,
    OpenJupyterBrowser,
}

#[derive(Debug, Clone, Copy)]
struct CommandPaletteItem {
    label: &'static str,
    keywords: &'static str,
    action: CommandPaletteAction,
}

fn command_palette_items(service_running: bool) -> Vec<CommandPaletteItem> {
    let mut items = vec![
        CommandPaletteItem {
            label: "Open Dashboard",
            keywords: "screen ops dashboard runtime",
            action: CommandPaletteAction::OpenScreen(Screen::Dashboard),
        },
        CommandPaletteItem {
            label: "Open Devices",
            keywords: "screen ops devices streams",
            action: CommandPaletteAction::OpenScreen(Screen::Devices),
        },
        CommandPaletteItem {
            label: "Open Profiles",
            keywords: "screen ops profiles calibration",
            action: CommandPaletteAction::OpenScreen(Screen::Profiles),
        },
        CommandPaletteItem {
            label: "Open Visualization",
            keywords: "screen analysis visualization telemetry",
            action: CommandPaletteAction::OpenScreen(Screen::Visualization),
        },
        CommandPaletteItem {
            label: "Open Python Lab",
            keywords: "screen labs python notebook kernel",
            action: CommandPaletteAction::OpenScreen(Screen::PythonLab),
        },
        CommandPaletteItem {
            label: "Open Jupyter IDE",
            keywords: "screen labs jupyter ide",
            action: CommandPaletteAction::OpenScreen(Screen::JupyterIde),
        },
        CommandPaletteItem {
            label: "Open Settings",
            keywords: "screen config settings",
            action: CommandPaletteAction::OpenScreen(Screen::Settings),
        },
        CommandPaletteItem {
            label: "Toggle Bottom Panel",
            keywords: "panel bottom toggle",
            action: CommandPaletteAction::ToggleBottomPanel,
        },
        CommandPaletteItem {
            label: "Focus Runtime Panel",
            keywords: "runtime panel tab",
            action: CommandPaletteAction::OpenBottomTab(BottomTab::Runtime),
        },
        CommandPaletteItem {
            label: "Focus Problems Panel",
            keywords: "problems panel errors",
            action: CommandPaletteAction::OpenBottomTab(BottomTab::Problems),
        },
        CommandPaletteItem {
            label: "Focus Logs Panel",
            keywords: "logs panel logger",
            action: CommandPaletteAction::OpenBottomTab(BottomTab::Logs),
        },
        CommandPaletteItem {
            label: "Focus Console Panel",
            keywords: "console panel stream",
            action: CommandPaletteAction::OpenBottomTab(BottomTab::Console),
        },
        CommandPaletteItem {
            label: "Reconnect Bridge",
            keywords: "runtime bridge reconnect recover",
            action: CommandPaletteAction::ReconnectBridge,
        },
        CommandPaletteItem {
            label: "Refresh Runtime Snapshot",
            keywords: "runtime snapshot refresh status",
            action: CommandPaletteAction::RefreshRuntimeSnapshot,
        },
        CommandPaletteItem {
            label: "Apply Fallback Policy",
            keywords: "runtime fallback policy",
            action: CommandPaletteAction::ApplyFallbackPolicy,
        },
        CommandPaletteItem {
            label: "Prepare Jupyter Environment",
            keywords: "jupyter bootstrap prepare environment",
            action: CommandPaletteAction::PrepareEnvironment,
        },
        CommandPaletteItem {
            label: "Start Jupyter",
            keywords: "jupyter start run",
            action: CommandPaletteAction::StartJupyter,
        },
        CommandPaletteItem {
            label: "Stop Jupyter",
            keywords: "jupyter stop",
            action: CommandPaletteAction::StopJupyter,
        },
        CommandPaletteItem {
            label: "Open Jupyter in Browser",
            keywords: "jupyter browser open",
            action: CommandPaletteAction::OpenJupyterBrowser,
        },
    ];

    items.push(if service_running {
        CommandPaletteItem {
            label: "Stop Service",
            keywords: "runtime service stop shutdown",
            action: CommandPaletteAction::StopService,
        }
    } else {
        CommandPaletteItem {
            label: "Start Service",
            keywords: "runtime service start",
            action: CommandPaletteAction::StartService,
        }
    });

    items
}

fn advanced_status_bar_tabs() -> [BottomTab; 4] {
    [
        BottomTab::Problems,
        BottomTab::Runtime,
        BottomTab::Logs,
        BottomTab::Console,
    ]
}

fn advanced_bottom_panel_tabs() -> [BottomTab; 4] {
    [
        BottomTab::Problems,
        BottomTab::Runtime,
        BottomTab::Logs,
        BottomTab::Console,
    ]
}

#[cfg(test)]
mod tests {
    use armas::components::SidebarState;
    use egui_kittest::{Harness, kittest::Queryable};
    use neurohid_types::config::UiMode;

    use crate::screens::Screen;
    use crate::workbench::{ActivityLane, BottomTab, WorkbenchState, screens_for_lane};

    use super::{
        ProblemRow, SidebarShellResponse, advanced_bottom_panel_tabs, advanced_status_bar_tabs,
        apply_sidebar_shell_response, build_device_health_problem_row, command_palette_items,
        normalize_problem_rows, problem_severity_rank, render_sidebar_shell,
    };

    struct SidebarHarnessState {
        sidebar_state: SidebarState,
    }

    #[test]
    fn expanded_sidebar_renders_navigation_without_runtime_section() {
        let harness = Harness::new_ui_state(
            |ui, state: &mut SidebarHarnessState| {
                let _ = render_sidebar_shell(
                    ui,
                    &mut state.sidebar_state,
                    Screen::all_for_mode(&UiMode::Advanced),
                    Screen::Dashboard,
                    ActivityLane::Ops,
                    None,
                );
            },
            SidebarHarnessState {
                sidebar_state: SidebarState::new(true),
            },
        );

        assert!(harness.query_all_by_label("NeuroHID").next().is_none());
        assert!(harness.query_all_by_label("Platform").next().is_some());
        assert!(harness.query_all_by_label("Dashboard").next().is_some());
        assert!(harness.query_all_by_label("Runtime").next().is_none());
        assert!(
            harness
                .query_all_by_label("Service: Running")
                .next()
                .is_none()
        );
        assert!(
            harness
                .query_all_by_label("IPC: Connected")
                .next()
                .is_none()
        );
    }

    #[test]
    fn sidebar_keeps_single_devices_navigation_entry() {
        let harness = Harness::new_ui_state(
            |ui, state: &mut SidebarHarnessState| {
                let _ = render_sidebar_shell(
                    ui,
                    &mut state.sidebar_state,
                    Screen::all_for_mode(&UiMode::Advanced),
                    Screen::Dashboard,
                    ActivityLane::Ops,
                    None,
                );
            },
            SidebarHarnessState {
                sidebar_state: SidebarState::new(true),
            },
        );

        assert_eq!(harness.query_all_by_label("Devices").count(), 1);
    }

    #[test]
    fn labs_lane_sidebar_scopes_to_lab_screens() {
        let harness = Harness::new_ui_state(
            |ui, state: &mut SidebarHarnessState| {
                let _ = render_sidebar_shell(
                    ui,
                    &mut state.sidebar_state,
                    screens_for_lane(ActivityLane::Labs),
                    Screen::PythonLab,
                    ActivityLane::Labs,
                    None,
                );
            },
            SidebarHarnessState {
                sidebar_state: SidebarState::new(true),
            },
        );

        assert!(harness.query_all_by_label("Python Lab").next().is_some());
        assert!(harness.query_all_by_label("Jupyter IDE").next().is_some());
        assert!(harness.query_all_by_label("Dashboard").next().is_none());
    }

    #[test]
    fn workbench_bottom_tab_toggle_contract() {
        let mut workbench = WorkbenchState::default();

        workbench.toggle_bottom_tab(BottomTab::Runtime);
        assert!(workbench.bottom_panel.visible);
        assert_eq!(workbench.bottom_panel.active_tab, BottomTab::Runtime);

        workbench.toggle_bottom_tab(BottomTab::Runtime);
        assert!(!workbench.bottom_panel.visible);

        workbench.toggle_bottom_tab(BottomTab::Runtime);
        assert!(workbench.bottom_panel.visible);
        assert_eq!(workbench.bottom_panel.active_tab, BottomTab::Runtime);
    }

    #[test]
    fn sidebar_shows_keyboard_focus_hint_when_present() {
        let harness = Harness::new_ui_state(
            |ui, state: &mut SidebarHarnessState| {
                let _ = render_sidebar_shell(
                    ui,
                    &mut state.sidebar_state,
                    Screen::all_for_mode(&UiMode::Advanced),
                    Screen::Dashboard,
                    ActivityLane::Ops,
                    Some(Screen::Dashboard),
                );
            },
            SidebarHarnessState {
                sidebar_state: SidebarState::new(true),
            },
        );

        assert!(
            harness
                .query_all_by_label("Keyboard focus Dashboard")
                .next()
                .is_some()
        );
    }

    #[test]
    fn normalize_problem_rows_deduplicates_by_message_keeping_highest_severity() {
        let rows = vec![
            ProblemRow {
                message: "Runtime IPC disconnected".to_string(),
                intent: crate::theme::Intent::Warning,
                screen_target: Some(Screen::Dashboard),
                tab_target: Some(BottomTab::Runtime),
            },
            ProblemRow {
                message: "Runtime IPC disconnected".to_string(),
                intent: crate::theme::Intent::Danger,
                screen_target: Some(Screen::Devices),
                tab_target: Some(BottomTab::Problems),
            },
            ProblemRow {
                message: "Trainer error: timeout".to_string(),
                intent: crate::theme::Intent::Warning,
                screen_target: Some(Screen::PythonLab),
                tab_target: Some(BottomTab::Problems),
            },
        ];

        let normalized = normalize_problem_rows(rows);

        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].message, "Runtime IPC disconnected");
        assert_eq!(problem_severity_rank(normalized[0].intent), 4);
        assert_eq!(normalized[1].message, "Trainer error: timeout");
    }

    #[test]
    fn normalize_problem_rows_orders_by_severity_then_message() {
        let rows = vec![
            ProblemRow {
                message: "zeta warning".to_string(),
                intent: crate::theme::Intent::Warning,
                screen_target: None,
                tab_target: None,
            },
            ProblemRow {
                message: "alpha danger".to_string(),
                intent: crate::theme::Intent::Danger,
                screen_target: None,
                tab_target: None,
            },
            ProblemRow {
                message: "beta danger".to_string(),
                intent: crate::theme::Intent::Danger,
                screen_target: None,
                tab_target: None,
            },
        ];

        let normalized = normalize_problem_rows(rows);
        let ordered_messages: Vec<&str> =
            normalized.iter().map(|row| row.message.as_str()).collect();

        assert_eq!(
            ordered_messages,
            vec!["alpha danger", "beta danger", "zeta warning"]
        );
    }

    #[test]
    fn device_health_problem_row_collapses_multiple_stream_issues() {
        let mut snap = crate::state::ServiceSnapshot {
            running: true,
            ..Default::default()
        };

        snap.discovered_streams
            .push(neurohid_types::device::DiscoveredStream {
                id: "stream-1".to_string(),
                name: "Mock EEG".to_string(),
                stream_type: "EEG".to_string(),
                channel_count: 8,
                sample_rate: 256.0,
                connected: true,
                battery_percent: Some(18),
                channel_quality: Some(vec![0.42, 0.41, 0.44]),
                source_id: Some("mock-source".to_string()),
                effective_sample_rate_hz: None,
                samples_received: None,
                samples_dropped: None,
                drop_rate_pct: None,
                last_sample_age_ms: None,
                preprocessing_summary: None,
                integrity_state: None,
            });

        let row = build_device_health_problem_row(
            &snap,
            0,
            &neurohid_types::config::DeviceHealthProblemConfig::default(),
        )
        .expect("row should be synthesized");
        assert!(row.message.starts_with("Device health warning: "));
        assert!(
            row.message
                .contains("Connected streams have no active routes")
        );
        assert!(row.message.contains("(+1 more)"));
        assert_eq!(row.intent, crate::theme::Intent::Warning);
        assert_eq!(row.tab_target, Some(BottomTab::Runtime));
        assert_eq!(row.screen_target, Some(Screen::Devices));
    }

    #[test]
    fn device_health_problem_row_escalates_to_danger_for_critical_battery() {
        let mut snap = crate::state::ServiceSnapshot {
            running: true,
            ..Default::default()
        };

        snap.discovered_streams
            .push(neurohid_types::device::DiscoveredStream {
                id: "stream-critical-battery".to_string(),
                name: "Critical EEG".to_string(),
                stream_type: "EEG".to_string(),
                channel_count: 8,
                sample_rate: 256.0,
                connected: true,
                battery_percent: Some(9),
                channel_quality: Some(vec![0.8, 0.82, 0.79]),
                source_id: Some("critical-source".to_string()),
                effective_sample_rate_hz: None,
                samples_received: None,
                samples_dropped: None,
                drop_rate_pct: None,
                last_sample_age_ms: None,
                preprocessing_summary: None,
                integrity_state: None,
            });

        let row = build_device_health_problem_row(
            &snap,
            1,
            &neurohid_types::config::DeviceHealthProblemConfig::default(),
        )
        .expect("row should be synthesized");
        assert!(row.message.starts_with("Device health critical: "));
        assert!(
            row.message
                .contains("Critical battery on Critical EEG (9%)")
        );
        assert_eq!(row.intent, crate::theme::Intent::Danger);
        assert_eq!(row.tab_target, Some(BottomTab::Problems));
        assert_eq!(row.screen_target, Some(Screen::Devices));
    }

    #[test]
    fn device_health_problem_row_escalates_to_danger_for_critical_quality() {
        let mut snap = crate::state::ServiceSnapshot {
            running: true,
            ..Default::default()
        };

        snap.discovered_streams
            .push(neurohid_types::device::DiscoveredStream {
                id: "stream-critical-quality".to_string(),
                name: "Noisy EEG".to_string(),
                stream_type: "EEG".to_string(),
                channel_count: 8,
                sample_rate: 256.0,
                connected: true,
                battery_percent: Some(75),
                channel_quality: Some(vec![0.32, 0.31, 0.34]),
                source_id: Some("quality-source".to_string()),
                effective_sample_rate_hz: None,
                samples_received: None,
                samples_dropped: None,
                drop_rate_pct: None,
                last_sample_age_ms: None,
                preprocessing_summary: None,
                integrity_state: None,
            });

        let row = build_device_health_problem_row(
            &snap,
            1,
            &neurohid_types::config::DeviceHealthProblemConfig::default(),
        )
        .expect("row should be synthesized");
        assert!(row.message.starts_with("Device health critical: "));
        assert!(
            row.message
                .contains("Critical channel quality on Noisy EEG (32%)")
        );
        assert_eq!(row.intent, crate::theme::Intent::Danger);
        assert_eq!(row.tab_target, Some(BottomTab::Runtime));
        assert_eq!(row.screen_target, Some(Screen::Devices));
    }

    #[test]
    fn sidebar_renders_lane_strip_entries() {
        let harness = Harness::new_ui_state(
            |ui, state: &mut SidebarHarnessState| {
                let _ = render_sidebar_shell(
                    ui,
                    &mut state.sidebar_state,
                    Screen::all_for_mode(&UiMode::Advanced),
                    Screen::Dashboard,
                    ActivityLane::Ops,
                    None,
                );
            },
            SidebarHarnessState {
                sidebar_state: SidebarState::new(true),
            },
        );

        assert!(harness.query_all_by_label("Ops").next().is_some());
        assert!(harness.query_all_by_label("Analysis").next().is_some());
        assert!(harness.query_all_by_label("Labs").next().is_some());
        assert!(harness.query_all_by_label("Settings").next().is_some());
    }

    #[test]
    fn sidebar_lane_selection_action_switches_screen_scope() {
        let mut workbench = WorkbenchState::default();
        let mut current_screen = Screen::Dashboard;

        apply_sidebar_shell_response(
            &UiMode::Advanced,
            SidebarShellResponse {
                lane_selection: Some(ActivityLane::Labs),
                ..SidebarShellResponse::default()
            },
            &mut workbench,
            &mut current_screen,
        );

        assert_eq!(workbench.lane, ActivityLane::Labs);
        assert_eq!(current_screen, Screen::PythonLab);
    }

    #[test]
    fn sidebar_settings_action_opens_config_lane() {
        let mut workbench = WorkbenchState::default();
        let mut current_screen = Screen::Dashboard;

        apply_sidebar_shell_response(
            &UiMode::Advanced,
            SidebarShellResponse {
                open_settings: true,
                ..SidebarShellResponse::default()
            },
            &mut workbench,
            &mut current_screen,
        );

        assert_eq!(workbench.lane, ActivityLane::Config);
        assert_eq!(current_screen, Screen::Settings);
    }

    #[test]
    fn command_palette_exposes_runtime_quick_actions() {
        let labels: Vec<&str> = command_palette_items(true)
            .into_iter()
            .map(|item| item.label)
            .collect();

        assert!(labels.contains(&"Reconnect Bridge"));
        assert!(labels.contains(&"Refresh Runtime Snapshot"));
        assert!(labels.contains(&"Apply Fallback Policy"));
    }

    #[test]
    fn advanced_status_and_bottom_tabs_share_order() {
        let status_tabs: Vec<&str> = advanced_status_bar_tabs()
            .into_iter()
            .map(BottomTab::label)
            .collect();
        let bottom_tabs: Vec<&str> = advanced_bottom_panel_tabs()
            .into_iter()
            .map(BottomTab::label)
            .collect();

        let expected = vec!["Problems", "Runtime", "Logs", "Console"];
        assert_eq!(status_tabs, expected);
        assert_eq!(bottom_tabs, expected);
    }
}
