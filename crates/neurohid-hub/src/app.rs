//! # Hub Application
//!
//! The main `eframe::App` implementation that ties together the sidebar,
//! status bar, and screen dispatch.

use eframe::egui;

use crate::state::HubState;
use crate::service_manager::ServiceManager;
use crate::screens::Screen;
use crate::screens::dashboard::DashboardScreen;
use crate::screens::devices::DevicesScreen;
use crate::screens::profiles::ProfilesScreen;
use crate::screens::calibration::CalibrationScreen;
use crate::screens::settings::SettingsScreen;

/// The main hub application.
pub struct HubApp {
    runtime: tokio::runtime::Runtime,
    current_screen: Screen,
    state: HubState,
    service_manager: ServiceManager,
    // Screen instances
    dashboard: DashboardScreen,
    devices: DevicesScreen,
    profiles: ProfilesScreen,
    calibration: CalibrationScreen,
    settings: SettingsScreen,
}

impl HubApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, runtime: tokio::runtime::Runtime) -> Self {
        // Initialize storage (blocking on the runtime since we're in the main thread)
        let (state, init_error) = match runtime.block_on(Self::init_state()) {
            Ok(state) => (state, None),
            Err(e) => {
                let error_msg = format!("{}", e);
                tracing::error!("Failed to initialize: {}", error_msg);
                // Create a minimal fallback state
                let fallback = runtime.block_on(async {
                    let config = neurohid_types::config::SystemConfig::default();
                    let (ps, cs) = neurohid_storage::initialize().await
                        .unwrap_or_else(|_| {
                            panic!("Cannot initialize storage at all")
                        });
                    HubState::new(ps, cs, config, vec![])
                });
                (fallback, Some(error_msg))
            }
        };

        let mut hub = Self {
            runtime,
            current_screen: Screen::Dashboard,
            state,
            service_manager: ServiceManager::new(),
            dashboard: DashboardScreen::new(),
            devices: DevicesScreen::new(),
            profiles: ProfilesScreen::new(),
            calibration: CalibrationScreen::new(),
            settings: SettingsScreen::new(),
        };

        if let Some(err) = init_error {
            hub.state.init_error = Some(err);
        }

        // Auto-start the service so streams are discovered immediately.
        let profile_store = Some(hub.state.profile_store.clone());
        let profile_id = hub.state.active_profile_id.clone();
        hub.service_manager.start(
            &hub.runtime,
            hub.state.config.clone(),
            profile_store,
            profile_id,
        );

        hub
    }

    async fn init_state() -> anyhow::Result<HubState> {
        let (profile_store, config_store) = neurohid_storage::initialize().await
            .map_err(|e| anyhow::anyhow!("Storage init failed: {}", e))?;

        let config = config_store.load().await
            .map_err(|e| anyhow::anyhow!("Config load failed: {}", e))?;

        let profiles = profile_store.list_profiles().await
            .unwrap_or_default();

        Ok(HubState::new(profile_store, config_store, config, profiles))
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
                for &screen in Screen::all() {
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
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::GREEN, "●");
                        let name = snap.device_name.as_deref().unwrap_or("Unknown");
                        ui.label(format!("Device: {}", name));
                    });
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

    fn show_status_bar(&self, ctx: &egui::Context) {
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
}

impl eframe::App for HubApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll service state (non-blocking)
        self.state.service_snapshot = self.service_manager.snapshot();

        // Show sidebar and status bar (always visible)
        self.show_sidebar(ctx);
        self.show_status_bar(ctx);

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
                    self.dashboard.show(ui, &self.state, &mut self.service_manager, &self.runtime);
                }
                Screen::Devices => {
                    self.devices.show(ui, &self.state, &mut self.service_manager);
                }
                Screen::Profiles => {
                    self.profiles.show(ui, &mut self.state, &self.runtime);
                }
                Screen::Calibration => {
                    self.calibration.show_entry(
                        ui,
                        &mut self.state,
                        &mut self.service_manager,
                    );
                }
                Screen::Settings => {
                    self.settings.show(ui, &mut self.state, &self.runtime);
                }
            }
        });

        // Request continuous repainting for live updates
        ctx.request_repaint();
    }
}
