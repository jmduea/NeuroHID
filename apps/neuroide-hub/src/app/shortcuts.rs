use super::HubApp;
use crate::workbench::ActivityLane;
use eframe::egui;
use neurohid_types::config::UiMode;

impl HubApp {
    pub(crate) fn focus_sidebar_navigation(&mut self) {
        self.sidebar_state.set_open(true);
        let screens = self.workbench.visible_screens(&self.state.config.ui.mode);
        if screens.is_empty() {
            return;
        }
        if !screens.contains(&self.current_screen) {
            self.current_screen = screens[0];
            self.workbench
                .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);
            self.persist_last_screen();
        }
        self.workbench.sidebar_focus_screen = Some(self.current_screen);
    }
    pub(crate) fn step_sidebar_navigation(&mut self, step: i32) {
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
        self.persist_last_screen();
        self.workbench.sidebar_focus_screen = Some(self.current_screen);
    }
    pub(crate) fn handle_workbench_shortcuts(&mut self, ctx: &egui::Context) {
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
                    Some(ActivityLane::Devices)
                } else if input.key_pressed(egui::Key::C) {
                    Some(ActivityLane::Calibration)
                } else if input.key_pressed(egui::Key::T) {
                    Some(ActivityLane::Training)
                } else if input.key_pressed(egui::Key::V) {
                    Some(ActivityLane::Visualization)
                } else if input.key_pressed(egui::Key::G) {
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
}
