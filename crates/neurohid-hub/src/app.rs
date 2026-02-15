//! # Hub Application
//!
//! The main `eframe::App` implementation that ties together the sidebar,
//! status bar, and screen dispatch.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use armas::components::{CollapsibleMode, Sidebar, SidebarState, SidebarVariant};
use eframe::egui;
use neurohid_types::control::RuntimeModeState;

use crate::data_bus::DataBus;
use crate::screens::Screen;
use crate::screens::calibration::CalibrationScreen;
use crate::screens::dashboard::DashboardScreen;
use crate::screens::devices::{DevicesScreen, derive_device_label};
use crate::screens::jupyter_ide::JupyterIdeScreen;
use crate::screens::profiles::ProfilesScreen;
use crate::screens::python_lab::PythonLabScreen;
use crate::screens::settings::SettingsScreen;
use crate::screens::visualization::VisualizationScreen;
use crate::service_manager::ServiceManager;
use crate::state::HubState;
use crate::stream_console::StreamConsole;
use crate::theme;

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
        let panel_width = self.sidebar_state.width().clamp(56.0, 272.0) + 16.0;
        egui::SidePanel::left("sidebar")
            .exact_width(panel_width)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("NeuroHID").heading().strong());
                ui.label(
                    egui::RichText::new("Neural Interaction Console")
                        .small()
                        .weak(),
                );
                ui.add_space(8.0);

                let screens = Screen::all_for_mode(&self.state.config.ui.mode);
                let snap = &self.state.service_snapshot;
                let service_text = if snap.running { "Running" } else { "Stopped" };

                let ipc_text = if snap.ipc_connected {
                    if snap.ipc_simulated {
                        "Simulated"
                    } else {
                        "Connected"
                    }
                } else {
                    "Disconnected"
                };
                let sidebar_response = Sidebar::new()
                    .state(&mut self.sidebar_state)
                    .variant(SidebarVariant::Floating)
                    .collapsible(CollapsibleMode::Icon)
                    .show(ui, |sidebar| {
                        sidebar.group_label("Platform");
                        for &screen in screens {
                            sidebar
                                .item(screen_glyph(screen), screen.label())
                                .active(self.current_screen == screen);
                        }

                        sidebar.group_label("Runtime");
                        sidebar.item("◉", &format!("Service: {service_text}"));
                        sidebar.item("◌", &format!("IPC: {ipc_text}"));

                        let routed_total = snap.routed_eeg_streams
                            + snap.routed_motion_streams
                            + snap.routed_auxiliary_streams
                            + snap.routed_unknown_streams;
                        if routed_total > 0 {
                            sidebar
                                .item("⟳", &format!("Routes: {routed_total}"))
                                .badge(routed_total.to_string());
                        }

                        if let Some((task, _)) = &snap.task_error {
                            sidebar.item("⚠", &format!("Error: {task} task"));
                        }

                        if snap.device_connected {
                            let mut groups: BTreeMap<
                                Option<String>,
                                Vec<&neurohid_types::device::DiscoveredStream>,
                            > = BTreeMap::new();
                            for stream in &snap.discovered_streams {
                                groups
                                    .entry(stream.source_id.clone())
                                    .or_default()
                                    .push(stream);
                            }

                            sidebar.group("⌁", "Devices", |group| {
                                for (source_id, streams) in &groups {
                                    let device_label = match source_id {
                                        Some(src_id) if streams.len() > 1 => {
                                            derive_device_label(streams, src_id)
                                        }
                                        _ => streams
                                            .first()
                                            .map(|stream| stream.name.clone())
                                            .unwrap_or_default(),
                                    };

                                    let connected =
                                        streams.iter().filter(|stream| stream.connected).count();
                                    let total = streams.len();
                                    group.group(
                                        "◍",
                                        &format!("{} ({}/{})", device_label, connected, total),
                                        |stream_group| {
                                            for stream in streams {
                                                stream_group.item(
                                                    "•",
                                                    &format!(
                                                        "{} · {} · {}ch · {:.0}Hz",
                                                        stream.name,
                                                        stream.stream_type,
                                                        stream.channel_count,
                                                        stream.sample_rate
                                                    ),
                                                );
                                            }
                                        },
                                    );
                                }
                            });
                        } else if snap.running {
                            sidebar.item("◍", "Device: Disconnected");
                        }
                    });

                if let Some(clicked_id) = sidebar_response.clicked {
                    for &screen in screens {
                        let nav_id = format!("item_0_{}", screen.label());
                        if clicked_id == nav_id {
                            self.current_screen = screen;
                            break;
                        }
                    }
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("v0.1.0").small().weak());
                });
            });
    }

    fn show_status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(30.0)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let snap = &self.state.service_snapshot;

                    let mut render_surface_controls = |ui: &mut egui::Ui| {
                        let console_label = if self.stream_console.visible {
                            "Console [x]"
                        } else {
                            "Console"
                        };
                        let console_clicked = theme::action_button(
                            ui,
                            console_label,
                            true,
                            if self.stream_console.visible {
                                theme::ButtonTone::Secondary
                            } else {
                                theme::ButtonTone::Ghost
                            },
                        );
                        if console_clicked {
                            self.stream_console.toggle();
                        }

                        let logs_label = if self.show_log_window {
                            "Logs [x]"
                        } else {
                            "Logs"
                        };
                        let logs_clicked = theme::action_button(
                            ui,
                            logs_label,
                            true,
                            if self.show_log_window {
                                theme::ButtonTone::Secondary
                            } else {
                                theme::ButtonTone::Ghost
                            },
                        );
                        if logs_clicked {
                            self.show_log_window = !self.show_log_window;
                        }
                    };

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
                    } else if let Some((task, _)) = &snap.task_error {
                        theme::status_chip(
                            ui,
                            &format!("Service stopped: {} task failed", task),
                            theme::Intent::Danger,
                        );
                    } else {
                        theme::status_chip(ui, "Service not running", theme::Intent::Muted);
                    }

                    ui.separator();
                    render_surface_controls(ui);
                });
            });
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

        // Keep manager mode/endpoint in sync with persisted runtime settings.
        self.service_manager.configure(&self.state.config);

        // Poll service state (non-blocking)
        self.state.service_snapshot = self.service_manager.snapshot();
        self.maybe_notify_latency_transition();
        self.maybe_notify_runtime_mode_transition();

        // Connect/disconnect the data bus based on service state
        self.service_manager.sync_data_bus(&mut self.data_bus);

        // Poll data bus — drain broadcast channels into ring buffers
        self.data_bus.poll();

        // Update stream console with new data
        self.stream_console
            .update(&self.data_bus, &self.state.service_snapshot);

        // Show sidebar and status bar (always visible)
        self.show_sidebar(ctx);
        self.show_status_bar(ctx);

        // Show stream console (renders before CentralPanel to claim bottom space)
        self.stream_console
            .show(ctx, &self.data_bus, &self.state.service_snapshot);

        if self.show_log_window {
            egui::Window::new("Runtime Logs")
                .open(&mut self.show_log_window)
                .default_size(egui::vec2(760.0, 320.0))
                .vscroll(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let clear_clicked =
                            theme::action_button(ui, "Clear", true, theme::ButtonTone::Secondary);
                        if clear_clicked {
                            egui_logger::clear_logs();
                        }
                    });
                    ui.separator();
                    egui_logger::logger_ui().show(ui);
                });
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
        Screen::Dashboard => "◉",
        Screen::Visualization => "◧",
        Screen::Devices => "⌁",
        Screen::Profiles => "◎",
        Screen::Calibration => "◍",
        Screen::JupyterIde => "⟡",
        Screen::PythonLab => "⌬",
        Screen::Settings => "⚙",
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
