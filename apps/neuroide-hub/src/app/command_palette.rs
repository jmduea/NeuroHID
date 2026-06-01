use super::HubApp;
use crate::screens::Screen;
use crate::workbench::BottomTab;
use eframe::egui;
use neurohid_types::config::UiMode;

impl HubApp {
    pub(crate) fn show_command_palette(&mut self, ctx: &egui::Context) {
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
                let actions = command_palette_items(
                    self.state.service_snapshot.running,
                    self.state.config.ui.visualization_detached,
                );
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
            self.execute_command_palette_action(ctx, action);
            self.workbench.command_palette_open = false;
            self.workbench.command_query.clear();
            self.workbench.command_palette_focus_index = 0;
        }
    }
    pub(crate) fn execute_command_palette_action(
        &mut self,
        ctx: &egui::Context,
        action: CommandPaletteAction,
    ) {
        match action {
            CommandPaletteAction::OpenScreen(screen) => {
                self.current_screen = screen;
                self.workbench
                    .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);
                self.persist_last_screen();
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
                self.persist_last_screen();
            }
            CommandPaletteAction::StartJupyter => {
                self.jupyter_ide
                    .command_start_jupyter(&self.state.config.ui);
                self.current_screen = Screen::JupyterIde;
                self.workbench
                    .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);
                self.persist_last_screen();
            }
            CommandPaletteAction::StopJupyter => {
                self.jupyter_ide.command_stop_jupyter();
                self.current_screen = Screen::JupyterIde;
                self.workbench
                    .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);
                self.persist_last_screen();
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
            CommandPaletteAction::ToggleVisualizationDetached => {
                self.toggle_visualization_detached(ctx);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CommandPaletteAction {
    OpenScreen(Screen),
    ToggleVisualizationDetached,
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
pub(crate) struct CommandPaletteItem {
    pub(crate) label: &'static str,
    pub(crate) keywords: &'static str,
    pub(crate) action: CommandPaletteAction,
}

pub(crate) fn command_palette_items(
    service_running: bool,
    visualization_detached: bool,
) -> Vec<CommandPaletteItem> {
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
            label: "Open Calibration",
            keywords: "screen calibration games wizard",
            action: CommandPaletteAction::OpenScreen(Screen::Calibration),
        },
        CommandPaletteItem {
            label: "Open Training",
            keywords: "screen training decoder config progress",
            action: CommandPaletteAction::OpenScreen(Screen::Training),
        },
        CommandPaletteItem {
            label: "Open Visualization",
            keywords: "screen analysis visualization telemetry",
            action: CommandPaletteAction::OpenScreen(Screen::Visualization),
        },
        CommandPaletteItem {
            label: if visualization_detached {
                "Attach Visualization"
            } else {
                "Detach Visualization"
            },
            keywords: "visualization window detach attach viewport",
            action: CommandPaletteAction::ToggleVisualizationDetached,
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
            label: "Open Extensions",
            keywords: "screen extensions plugins addons",
            action: CommandPaletteAction::OpenScreen(Screen::Extensions),
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
