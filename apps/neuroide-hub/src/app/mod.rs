//! # Hub Application
//!
//! The main `eframe::App` implementation that ties together the sidebar,
//! status bar, and screen dispatch.

use std::path::PathBuf;
use std::time::Duration;

use armas::components::SidebarState;
use eframe::egui;
use neurohid_types::config::UiMode;
use neurohid_types::control::RuntimeModeState;

use crate::data_bus::DataBus;
use crate::screens::Screen;
use crate::screens::calibration::CalibrationScreen;
use crate::screens::dashboard::DashboardScreen;
use crate::screens::devices::DevicesScreen;
use crate::screens::extensions::ExtensionsScreen;
use crate::screens::jupyter_ide::JupyterIdeScreen;
use crate::screens::profiles::ProfilesScreen;
use crate::screens::python_lab::PythonLabScreen;
use crate::screens::settings::SettingsScreen;
use crate::screens::training::TrainingScreen;
use crate::screens::visualization::VisualizationScreen;
use crate::service_manager::ServiceManager;
use crate::state::HubState;
use crate::stream_console::StreamConsole;
use crate::theme;
use crate::workbench::{BottomTab, WorkbenchState};

mod bottom_panel;
mod command_palette;
mod notifications;
mod shortcuts;
/// The main hub application.
mod sidebar;
mod status_bar;
mod viewport;
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
    visualization_detached_native_active: bool,
    visualization_detached_fallback_warning: bool,
    visualization_detached_geometry_dirty: bool,
    visualization_detached_last_persist_secs: f64,
    // Screen instances
    dashboard: DashboardScreen,
    visualization: VisualizationScreen,
    devices: DevicesScreen,
    profiles: ProfilesScreen,
    calibration: CalibrationScreen,
    training: TrainingScreen,
    jupyter_ide: JupyterIdeScreen,
    python_lab: PythonLabScreen,
    extensions: ExtensionsScreen,
    settings: SettingsScreen,
}

impl HubApp {
    const DETACHED_VISUALIZATION_VIEWPORT_ID_SEED: &'static str =
        "neurohid_visualization_detached_viewport";
    const DETACHED_VISUALIZATION_DEFAULT_WIDTH: f32 = 1280.0;
    const DETACHED_VISUALIZATION_DEFAULT_HEIGHT: f32 = 760.0;
    const DETACHED_VISUALIZATION_MIN_WIDTH: f32 = 720.0;
    const DETACHED_VISUALIZATION_MIN_HEIGHT: f32 = 420.0;

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

        let current_screen = state
            .config
            .ui
            .last_screen
            .as_deref()
            .and_then(Screen::from_id);
        let current_screen = current_screen
            .filter(|s| Screen::all_for_mode(&state.config.ui.mode).contains(s))
            .unwrap_or(Screen::Dashboard);

        let mut hub = Self {
            runtime,
            current_screen,
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
            visualization_detached_native_active: false,
            visualization_detached_fallback_warning: false,
            visualization_detached_geometry_dirty: false,
            visualization_detached_last_persist_secs: f64::NEG_INFINITY,
            dashboard: DashboardScreen::new(),
            visualization,
            devices: DevicesScreen::new(),
            profiles: ProfilesScreen::new(),
            calibration: CalibrationScreen::new(),
            training: TrainingScreen::new(),
            jupyter_ide: JupyterIdeScreen::new(),
            python_lab: PythonLabScreen::new(),
            extensions: ExtensionsScreen::new(),
            settings: SettingsScreen::new(),
        };

        if let Some(err) = init_error {
            hub.state.init_error = Some(err);
        }

        hub.workbench
            .sync_lane_from_screen(&hub.state.config.ui.mode, hub.current_screen);
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

        let mut needs_save = false;

        if !config.service.auto_start {
            config.service.auto_start = true;
            needs_save = true;
        }

        if needs_save && let Err(error) = config_store.save(&config).await {
            tracing::warn!(
                error = %error,
                "Failed to persist migrated config defaults"
            );
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

    fn persist_config(&mut self, reason: &str) -> bool {
        match self
            .runtime
            .block_on(self.state.config_store.save(&self.state.config))
        {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!("Failed to persist {}: {}", reason, error);
                false
            }
        }
    }

    /// Persist current screen to config for resume state; call when screen changes.
    fn persist_last_screen(&mut self) {
        self.state.config.ui.last_screen = Some(self.current_screen.id().to_string());
        let _ = self.persist_config("last_screen (resume state)");
    }
}

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
            self.persist_last_screen();
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

        self.show_detached_visualization_viewport(ctx);

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

                // Run / Visualization tabs: when on Dashboard or Visualization (Advanced mode),
                // allow switching between Run and Visualization without leaving the content area.
                if self.state.config.ui.mode == UiMode::Advanced
                    && matches!(
                        self.current_screen,
                        Screen::Dashboard | Screen::Visualization
                    )
                {
                    ui.horizontal(|ui| {
                        let run_selected = self.current_screen == Screen::Dashboard;
                        let viz_selected = self.current_screen == Screen::Visualization;
                        if ui.selectable_label(run_selected, "Run").clicked() && !run_selected {
                            self.current_screen = Screen::Dashboard;
                            self.workbench.sync_lane_from_screen(
                                &self.state.config.ui.mode,
                                self.current_screen,
                            );
                            self.persist_last_screen();
                        }
                        if ui.selectable_label(viz_selected, "Visualization").clicked()
                            && !viz_selected
                        {
                            self.current_screen = Screen::Visualization;
                            self.workbench.sync_lane_from_screen(
                                &self.state.config.ui.mode,
                                self.current_screen,
                            );
                            self.persist_last_screen();
                        }
                    });
                    ui.add_space(6.0);
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
                        if self.state.config.ui.visualization_detached
                            && self.visualization_detached_native_active
                        {
                            theme::page_header(
                                ui,
                                "Visualization",
                                "Live telemetry is rendering in the detached window",
                            );
                            theme::status_chip(
                                ui,
                                "Visualization detached to secondary viewport",
                                theme::Intent::Info,
                            );
                            ui.add_space(8.0);
                            if theme::action_button(
                                ui,
                                "Attach Visualization",
                                true,
                                theme::ButtonTone::Secondary,
                            ) {
                                self.set_visualization_detached(ctx, false);
                            }
                        } else {
                            if self.visualization_detached_fallback_warning {
                                theme::status_chip(
                                    ui,
                                    "Detached viewport unsupported in this environment; \
                                     showing embedded visualization",
                                    theme::Intent::Warning,
                                );
                                ui.separator();
                            }
                            let snapshot = self.state.service_snapshot.clone();
                            self.visualization.show(
                                ui,
                                &self.data_bus,
                                &snapshot,
                                &mut self.state,
                                &self.runtime,
                            );
                        }
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
                    Screen::Training => {
                        self.training
                            .show(ui, &self.state, &mut self.service_manager);
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
                    Screen::Extensions => {
                        self.extensions.show(ui, &self.state, &self.runtime);
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

        let visualization_animating = (self.current_screen == Screen::Visualization
            && !self.state.config.ui.visualization_detached)
            || self.visualization_detached_native_active;

        let frame_interval = if visualization_animating {
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

#[expect(
    dead_code,
    reason = "reserved for command palette or future screen selector"
)]
fn screen_glyph(screen: Screen) -> &'static str {
    match screen {
        Screen::Dashboard => "DB",
        Screen::Visualization => "VZ",
        Screen::Devices => "DV",
        Screen::Profiles => "PF",
        Screen::Calibration => "CL",
        Screen::Training => "TR",
        Screen::JupyterIde => "JP",
        Screen::PythonLab => "PY",
        Screen::Extensions => "EX",
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

#[cfg(test)]
mod tests {
    use armas::components::SidebarState;
    use egui_kittest::{Harness, kittest::Queryable};
    use neurohid_types::config::UiMode;

    use crate::screens::Screen;
    use crate::workbench::{ActivityLane, BottomTab, WorkbenchState, screens_for_lane};

    use super::bottom_panel::{
        ProblemRow, advanced_bottom_panel_tabs, advanced_status_bar_tabs,
        build_device_health_problem_row, normalize_problem_rows, problem_severity_rank,
    };
    use super::command_palette::command_palette_items;
    use super::sidebar::{
        SidebarShellResponse, apply_sidebar_shell_response, render_sidebar_shell,
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
                    ActivityLane::Config,
                    None,
                );
            },
            SidebarHarnessState {
                sidebar_state: SidebarState::new(true),
            },
        );

        assert!(harness.query_all_by_label("NeuroHID").next().is_none());
        assert!(harness.query_all_by_label("Lanes").next().is_some());
        assert!(harness.query_all_by_label("Devices").next().is_some());
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
                    screens_for_lane(ActivityLane::Devices),
                    Screen::Devices,
                    ActivityLane::Devices,
                    None,
                );
            },
            SidebarHarnessState {
                sidebar_state: SidebarState::new(true),
            },
        );

        // Devices lane + Devices screen entry in that lane
        assert!(harness.query_all_by_label("Devices").count() >= 1);
    }

    #[test]
    fn config_lane_sidebar_scopes_to_dashboard_profiles_settings_labs() {
        let harness = Harness::new_ui_state(
            |ui, state: &mut SidebarHarnessState| {
                let _ = render_sidebar_shell(
                    ui,
                    &mut state.sidebar_state,
                    screens_for_lane(ActivityLane::Config),
                    Screen::PythonLab,
                    ActivityLane::Config,
                    None,
                );
            },
            SidebarHarnessState {
                sidebar_state: SidebarState::new(true),
            },
        );

        // Sidebar shows lanes only (no duplicate Platform list); Config lane is present.
        assert!(harness.query_all_by_label("Config").next().is_some());
        assert!(harness.query_all_by_label("Lanes").next().is_some());
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
                    ActivityLane::Config,
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
                    Screen::Devices,
                    ActivityLane::Devices,
                    None,
                );
            },
            SidebarHarnessState {
                sidebar_state: SidebarState::new(true),
            },
        );

        assert!(harness.query_all_by_label("Devices").next().is_some());
        assert!(harness.query_all_by_label("Calibration").next().is_some());
        assert!(harness.query_all_by_label("Training").next().is_some());
        assert!(harness.query_all_by_label("Visualization").next().is_some());
        assert!(harness.query_all_by_label("Config").next().is_some());
        assert!(harness.query_all_by_label("Settings").next().is_some());
    }

    #[test]
    fn sidebar_lane_selection_action_switches_screen_scope() {
        let mut workbench = WorkbenchState::default();
        let mut current_screen = Screen::Dashboard;

        apply_sidebar_shell_response(
            &UiMode::Advanced,
            SidebarShellResponse {
                lane_selection: Some(ActivityLane::Config),
                ..SidebarShellResponse::default()
            },
            &mut workbench,
            &mut current_screen,
        );

        assert_eq!(workbench.lane, ActivityLane::Config);
        assert_eq!(current_screen, Screen::Dashboard);
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
        let labels: Vec<&str> = command_palette_items(true, false)
            .into_iter()
            .map(|item| item.label)
            .collect();

        assert!(labels.contains(&"Reconnect Bridge"));
        assert!(labels.contains(&"Refresh Runtime Snapshot"));
        assert!(labels.contains(&"Apply Fallback Policy"));
    }

    #[test]
    fn command_palette_detach_entry_reflects_detached_state() {
        let attached_labels: Vec<&str> = command_palette_items(true, false)
            .into_iter()
            .map(|item| item.label)
            .collect();
        let detached_labels: Vec<&str> = command_palette_items(true, true)
            .into_iter()
            .map(|item| item.label)
            .collect();

        assert!(attached_labels.contains(&"Detach Visualization"));
        assert!(detached_labels.contains(&"Attach Visualization"));
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
