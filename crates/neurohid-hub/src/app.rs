//! # Hub Application
//!
//! The main `eframe::App` implementation that ties together the sidebar,
//! status bar, and screen dispatch.

use std::collections::BTreeMap;
use std::path::PathBuf;

use eframe::egui;
use neurohid_types::control::RuntimeModeState;

use crate::data_bus::DataBus;
use crate::screens::calibration::CalibrationScreen;
use crate::screens::dashboard::DashboardScreen;
use crate::screens::devices::{derive_device_label, DevicesScreen};
use crate::screens::profiles::ProfilesScreen;
use crate::screens::python_lab::PythonLabScreen;
use crate::screens::settings::SettingsScreen;
use crate::screens::visualization::VisualizationScreen;
use crate::screens::Screen;
use crate::service_manager::ServiceManager;
use crate::state::HubState;
use crate::stream_console::StreamConsole;

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
    // Screen instances
    dashboard: DashboardScreen,
    visualization: VisualizationScreen,
    devices: DevicesScreen,
    profiles: ProfilesScreen,
    calibration: CalibrationScreen,
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
            dashboard: DashboardScreen::new(),
            visualization: VisualizationScreen::new(),
            devices: DevicesScreen::new(),
            profiles: ProfilesScreen::new(),
            calibration: CalibrationScreen::new(),
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

        let config = config_store
            .load()
            .await
            .map_err(|e| anyhow::anyhow!("Config load failed: {}", e))?;

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
        egui::SidePanel::left("sidebar")
            .exact_width(220.0)
            .show(ctx, |ui| {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| {
                    ui.heading("NeuroHID");
                });
                ui.add_space(16.0);

                ui.separator();
                ui.add_space(8.0);

                // Navigation items
                for &screen in Screen::all_for_mode(&self.state.config.ui.mode) {
                    let selected = self.current_screen == screen;
                    if ui.selectable_label(selected, screen.label()).clicked() {
                        self.current_screen = screen;
                    }
                }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                // Service status indicator
                let snap = &self.state.service_snapshot;
                let (status_color, status_text) = if snap.running {
                    (egui::Color32::GREEN, "Running")
                } else {
                    (egui::Color32::GRAY, "Stopped")
                };
                ui.horizontal(|ui| {
                    ui.colored_label(status_color, "●");
                    ui.label(format!("Service: {}", status_text));
                });

                let (ipc_color, ipc_text) = if snap.ipc_connected {
                    if snap.ipc_simulated {
                        (egui::Color32::YELLOW, "Simulated")
                    } else {
                        (egui::Color32::GREEN, "Connected")
                    }
                } else {
                    (egui::Color32::GRAY, "Disconnected")
                };
                ui.horizontal(|ui| {
                    ui.colored_label(ipc_color, "●");
                    ui.label(format!("IPC: {}", ipc_text));
                });

                if let Some((task, _)) = &snap.task_error {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::RED, "●");
                        ui.label(
                            egui::RichText::new(format!("Error: {} task", task))
                                .color(egui::Color32::RED)
                                .small(),
                        );
                    });
                }

                if snap.device_connected {
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Devices")
                            .small()
                            .strong()
                            .color(egui::Color32::LIGHT_GRAY),
                    );

                    // Group streams by source_id for collapsible device tree
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

                    for (source_id, streams) in &groups {
                        let device_label = match source_id {
                            Some(src_id) if streams.len() > 1 => {
                                derive_device_label(streams, src_id)
                            }
                            _ => {
                                // Single stream or no source_id — use the stream name
                                streams.first().map(|s| s.name.clone()).unwrap_or_default()
                            }
                        };

                        let connected = streams.iter().filter(|s| s.connected).count();
                        let total = streams.len();
                        let header_color = if connected == total {
                            egui::Color32::GREEN
                        } else if connected > 0 {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::GRAY
                        };

                        let header_id =
                            ui.make_persistent_id(source_id.as_deref().unwrap_or(&device_label));
                        egui::collapsing_header::CollapsingState::load_with_default_open(
                            ui.ctx(),
                            header_id,
                            false,
                        )
                        .show_header(ui, |ui| {
                            ui.colored_label(header_color, "●");
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} ({}/{})",
                                    device_label, connected, total
                                ))
                                .small(),
                            );
                        })
                        .body(|ui| {
                            for stream in streams {
                                let s_color = if stream.connected {
                                    egui::Color32::GREEN
                                } else {
                                    egui::Color32::GRAY
                                };
                                ui.horizontal(|ui| {
                                    ui.add_space(8.0);
                                    ui.colored_label(s_color, "○");
                                    ui.label(egui::RichText::new(&stream.name).small());
                                });
                                ui.horizontal(|ui| {
                                    ui.add_space(20.0);
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} · {}ch · {:.0}Hz",
                                            stream.stream_type,
                                            stream.channel_count,
                                            stream.sample_rate
                                        ))
                                        .small()
                                        .color(egui::Color32::GRAY),
                                    );
                                });
                            }
                        });
                    }
                } else if snap.running {
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::YELLOW, "●");
                        ui.label("Device: Disconnected");
                    });
                }

                // Fill remaining space so the version label is at the bottom
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("v0.1.0")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                });
            });
    }

    fn show_status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(28.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    let snap = &self.state.service_snapshot;

                    if snap.running {
                        ui.label(format!("Signal: {:.0}%", snap.signal_quality * 100.0));
                        ui.separator();
                        ui.label(format!("Actions: {}", snap.actions_emitted));
                        ui.separator();
                        ui.label(format!("Errors: {}", snap.errors_detected));
                        ui.separator();
                        let mins = snap.uptime_secs / 60;
                        let secs = snap.uptime_secs % 60;
                        ui.label(format!("Uptime: {}:{:02}", mins, secs));

                        if snap.calibration_mode {
                            ui.separator();
                            ui.colored_label(egui::Color32::YELLOW, "CALIBRATING");
                        }

                        ui.separator();
                        let console_label = if self.stream_console.visible {
                            "Console [x]"
                        } else {
                            "Console"
                        };
                        if ui.small_button(console_label).clicked() {
                            self.stream_console.toggle();
                        }
                    } else if let Some((task, _)) = &snap.task_error {
                        ui.colored_label(
                            egui::Color32::RED,
                            format!("Service stopped: {} task failed", task),
                        );
                    } else {
                        ui.label("Service not running");
                    }
                });
            });
    }

    fn apply_ui_preferences(&self, ctx: &egui::Context) {
        let ui_cfg = &self.state.config.ui;
        ctx.set_pixels_per_point(ui_cfg.font_scale.clamp(0.75, 2.0));

        match ui_cfg.theme_mode {
            neurohid_types::config::ThemeMode::Light => ctx.set_visuals(egui::Visuals::light()),
            neurohid_types::config::ThemeMode::Dark => ctx.set_visuals(egui::Visuals::dark()),
            neurohid_types::config::ThemeMode::System => {}
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

impl eframe::App for HubApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_ui_preferences(ctx);

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
            ctx.request_repaint();
            return;
        }

        // Central panel — dispatch to the active screen
        egui::CentralPanel::default().show(ctx, |ui| {
            // Show init error if any
            if let Some(err) = &self.state.init_error {
                ui.colored_label(egui::Color32::RED, format!("Init error: {}", err));
                ui.separator();
            }

            match self.current_screen {
                Screen::Dashboard => {
                    self.dashboard
                        .show(ui, &self.state, &mut self.service_manager, &self.runtime);
                }
                Screen::Visualization => {
                    self.visualization
                        .show(ui, &self.data_bus, &self.state.service_snapshot);
                }
                Screen::Devices => {
                    self.devices
                        .show(ui, &self.state, &mut self.service_manager);
                }
                Screen::Profiles => {
                    self.profiles
                        .show(ui, &mut self.state, &self.runtime, &self.service_manager);
                }
                Screen::Calibration => {
                    self.calibration
                        .show_entry(ui, &mut self.state, &mut self.service_manager);
                }
                Screen::PythonLab => {
                    self.python_lab.show(
                        ui,
                        &self.state.config.ui.lab_kernel_command,
                        &self.data_bus,
                        &self.state.service_snapshot,
                    );
                }
                Screen::Settings => {
                    self.settings
                        .show(ui, &mut self.state, &self.service_manager, &self.runtime);
                }
            }
        });

        // Request continuous repainting for live updates
        ctx.request_repaint();
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
        return desktop_notify_windows(title, body);
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
