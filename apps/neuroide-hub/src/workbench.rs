//! # Workbench State
//!
//! VSCode-inspired shell state used by Advanced mode.

use std::collections::VecDeque;

use neurohid_types::config::UiMode;
use neurohid_types::control::RuntimeModeState;

use crate::screens::Screen;
use crate::state::ServiceSnapshot;

/// Top-level sidebar lanes aligned to Phase 5 CONTEXT: Devices, Calibration,
/// Training, Visualization as primary; Config for Dashboard, Profiles, Settings, Labs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityLane {
    Devices,
    Calibration,
    Training,
    Visualization,
    Config,
}

impl ActivityLane {
    pub const ALL: [Self; 5] = [
        Self::Devices,
        Self::Calibration,
        Self::Training,
        Self::Visualization,
        Self::Config,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Devices => "Devices",
            Self::Calibration => "Calibration",
            Self::Training => "Training",
            Self::Visualization => "Visualization",
            Self::Config => "Config",
        }
    }

    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Devices => "DV",
            Self::Calibration => "CL",
            Self::Training => "TR",
            Self::Visualization => "VZ",
            Self::Config => "CF",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BottomTab {
    Console,
    Logs,
    Runtime,
    Problems,
}

impl BottomTab {
    pub const ALL: [Self; 4] = [Self::Console, Self::Logs, Self::Runtime, Self::Problems];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Console => "Console",
            Self::Logs => "Logs",
            Self::Runtime => "Runtime",
            Self::Problems => "Problems",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BottomPanelState {
    pub visible: bool,
    pub active_tab: BottomTab,
    pub height: f32,
}

impl Default for BottomPanelState {
    fn default() -> Self {
        Self {
            visible: false,
            active_tab: BottomTab::Runtime,
            height: 220.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeEvent {
    pub timestamp_secs: f64,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct WorkbenchState {
    pub lane: ActivityLane,
    pub bottom_panel: BottomPanelState,
    pub command_palette_open: bool,
    pub command_query: String,
    pub command_palette_focus_index: usize,
    pub sidebar_focus_screen: Option<Screen>,
    runtime_events: VecDeque<RuntimeEvent>,
    last_running: Option<bool>,
    last_bridge_connected: Option<bool>,
    last_runtime_mode: Option<RuntimeModeState>,
    last_task_error: Option<String>,
}

impl Default for WorkbenchState {
    fn default() -> Self {
        Self {
            lane: ActivityLane::Devices,
            bottom_panel: BottomPanelState::default(),
            command_palette_open: false,
            command_query: String::new(),
            command_palette_focus_index: 0,
            sidebar_focus_screen: None,
            runtime_events: VecDeque::new(),
            last_running: None,
            last_bridge_connected: None,
            last_runtime_mode: None,
            last_task_error: None,
        }
    }
}

impl WorkbenchState {
    const MAX_RUNTIME_EVENTS: usize = 80;

    pub fn visible_screens<'a>(&self, mode: &'a UiMode) -> &'a [Screen] {
        if *mode == UiMode::Advanced {
            screens_for_lane(self.lane)
        } else {
            Screen::all_for_mode(mode)
        }
    }

    pub fn sync_lane_from_screen(&mut self, mode: &UiMode, current_screen: Screen) {
        if *mode == UiMode::Advanced {
            self.lane = lane_for_screen(current_screen);
            if self.sidebar_focus_screen.is_some() {
                self.sidebar_focus_screen = Some(current_screen);
            }
        }
    }

    pub fn set_lane(&mut self, mode: &UiMode, lane: ActivityLane, current_screen: &mut Screen) {
        if *mode != UiMode::Advanced {
            return;
        }

        self.lane = lane;
        if !screens_for_lane(lane).contains(current_screen)
            && let Some(&first) = screens_for_lane(lane).first()
        {
            *current_screen = first;
        }
        if self.sidebar_focus_screen.is_some() {
            self.sidebar_focus_screen = Some(*current_screen);
        }
    }

    pub fn open_bottom_tab(&mut self, tab: BottomTab) {
        self.bottom_panel.visible = true;
        self.bottom_panel.active_tab = tab;
    }

    pub fn toggle_bottom_tab(&mut self, tab: BottomTab) {
        if self.bottom_panel.visible && self.bottom_panel.active_tab == tab {
            self.bottom_panel.visible = false;
            return;
        }
        self.open_bottom_tab(tab);
    }

    pub fn cycle_bottom_tab(&mut self, step: i32) {
        let tabs = BottomTab::ALL;
        let current = tabs
            .iter()
            .position(|tab| *tab == self.bottom_panel.active_tab)
            .unwrap_or(0) as i32;
        let len = tabs.len() as i32;
        let next = (current + step).rem_euclid(len) as usize;
        self.open_bottom_tab(tabs[next]);
    }

    pub fn runtime_events(&self) -> impl Iterator<Item = &RuntimeEvent> {
        self.runtime_events.iter().rev()
    }

    pub fn record_runtime_events(&mut self, snapshot: &ServiceSnapshot, now_secs: f64) {
        if let Some(previous) = self.last_running
            && previous != snapshot.running
        {
            self.push_runtime_event(
                now_secs,
                if snapshot.running {
                    "Service started".to_string()
                } else {
                    "Service stopped".to_string()
                },
            );
        }
        self.last_running = Some(snapshot.running);

        if let Some(previous) = self.last_bridge_connected
            && previous != snapshot.ml_bridge_connected
        {
            self.push_runtime_event(
                now_secs,
                if snapshot.ml_bridge_connected {
                    "ML bridge connected".to_string()
                } else {
                    "ML bridge disconnected".to_string()
                },
            );
        }
        self.last_bridge_connected = Some(snapshot.ml_bridge_connected);

        if let Some(previous) = self.last_runtime_mode
            && previous != snapshot.runtime_mode_state
        {
            self.push_runtime_event(
                now_secs,
                format!("Runtime mode -> {:?}", snapshot.runtime_mode_state),
            );
        }
        self.last_runtime_mode = Some(snapshot.runtime_mode_state);

        let current_task_error = snapshot
            .task_error
            .as_ref()
            .map(|(task, error)| format!("{task}: {error}"));
        if self.last_task_error != current_task_error {
            if let Some(error) = &current_task_error {
                self.push_runtime_event(now_secs, format!("Task error: {error}"));
            } else if self.last_task_error.is_some() {
                self.push_runtime_event(now_secs, "Task error cleared".to_string());
            }
        }
        self.last_task_error = current_task_error;
    }

    fn push_runtime_event(&mut self, now_secs: f64, message: String) {
        if self.runtime_events.len() == Self::MAX_RUNTIME_EVENTS {
            let _ = self.runtime_events.pop_front();
        }
        self.runtime_events.push_back(RuntimeEvent {
            timestamp_secs: now_secs,
            message,
        });
    }
}

pub fn screens_for_lane(lane: ActivityLane) -> &'static [Screen] {
    match lane {
        ActivityLane::Devices => &[Screen::Devices],
        ActivityLane::Calibration => &[Screen::Calibration],
        ActivityLane::Training => &[Screen::Training],
        ActivityLane::Visualization => &[Screen::Visualization],
        ActivityLane::Config => &[
            Screen::Dashboard,
            Screen::Profiles,
            Screen::Extensions,
            Screen::Settings,
            Screen::PythonLab,
            Screen::JupyterIde,
        ],
    }
}

pub const fn lane_for_screen(screen: Screen) -> ActivityLane {
    match screen {
        Screen::Devices => ActivityLane::Devices,
        Screen::Calibration => ActivityLane::Calibration,
        Screen::Training => ActivityLane::Training,
        Screen::Visualization => ActivityLane::Visualization,
        Screen::Dashboard
        | Screen::Profiles
        | Screen::Extensions
        | Screen::Settings
        | Screen::PythonLab
        | Screen::JupyterIde => ActivityLane::Config,
    }
}

#[cfg(test)]
mod tests {
    use neurohid_types::config::UiMode;

    use crate::state::ServiceSnapshot;

    use super::{ActivityLane, BottomTab, WorkbenchState, lane_for_screen, screens_for_lane};

    #[test]
    fn lane_screen_mapping_matches_contract() {
        assert_eq!(
            screens_for_lane(ActivityLane::Devices),
            &[crate::screens::Screen::Devices]
        );
        assert_eq!(
            screens_for_lane(ActivityLane::Calibration),
            &[crate::screens::Screen::Calibration]
        );
        assert_eq!(
            screens_for_lane(ActivityLane::Training),
            &[crate::screens::Screen::Training]
        );
        assert_eq!(
            screens_for_lane(ActivityLane::Visualization),
            &[crate::screens::Screen::Visualization]
        );
        assert_eq!(
            lane_for_screen(crate::screens::Screen::Training),
            ActivityLane::Training
        );
        assert_eq!(
            lane_for_screen(crate::screens::Screen::Visualization),
            ActivityLane::Visualization
        );
    }

    #[test]
    fn bottom_tab_toggle_hides_when_same_tab_clicked() {
        let mut state = WorkbenchState::default();

        state.toggle_bottom_tab(BottomTab::Console);
        assert!(state.bottom_panel.visible);
        assert_eq!(state.bottom_panel.active_tab, BottomTab::Console);

        state.toggle_bottom_tab(BottomTab::Console);
        assert!(!state.bottom_panel.visible);
    }

    #[test]
    fn advanced_visible_screens_are_lane_scoped() {
        let mut state = WorkbenchState::default();
        state.lane = ActivityLane::Config;

        assert!(
            state
                .visible_screens(&UiMode::Advanced)
                .contains(&crate::screens::Screen::Settings)
        );
        assert!(state.visible_screens(&UiMode::Standard).len() > 1);
    }

    #[test]
    fn runtime_events_capture_mode_and_bridge_changes() {
        let mut state = WorkbenchState::default();
        let mut snapshot = ServiceSnapshot::default();

        state.record_runtime_events(&snapshot, 1.0);
        snapshot.running = true;
        snapshot.ml_bridge_connected = true;
        snapshot.runtime_mode_state = neurohid_types::control::RuntimeModeState::Full;
        state.record_runtime_events(&snapshot, 2.0);

        let events: Vec<_> = state
            .runtime_events()
            .map(|event| event.message.clone())
            .collect();
        assert!(
            events
                .iter()
                .any(|message| message.contains("Service started"))
        );
        assert!(
            events
                .iter()
                .any(|message| message.contains("ML bridge connected"))
        );
    }
}
