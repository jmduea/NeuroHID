//! # Hub Screens
//!
//! Each screen renders into the hub's central panel area. The active screen
//! is determined by sidebar navigation in `app.rs`.

use neurohid_types::config::UiMode;

pub mod calibration;
pub mod dashboard;
pub mod devices;
pub mod profiles;
pub mod python_lab;
pub mod settings;
pub mod visualization;

/// The available hub screens, selected via sidebar navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Visualization,
    Devices,
    Profiles,
    Calibration,
    PythonLab,
    Settings,
}

impl Screen {
    pub fn label(&self) -> &'static str {
        match self {
            Screen::Dashboard => "Dashboard",
            Screen::Visualization => "Visualization",
            Screen::Devices => "Devices",
            Screen::Profiles => "Profiles",
            Screen::Calibration => "Calibration",
            Screen::PythonLab => "Python Lab",
            Screen::Settings => "Settings",
        }
    }

    pub fn all_for_mode(mode: &UiMode) -> &'static [Screen] {
        match mode {
            UiMode::Standard => &[
                Screen::Dashboard,
                Screen::Devices,
                Screen::Profiles,
                Screen::Calibration,
                Screen::Settings,
            ],
            UiMode::Advanced => &[
                Screen::Dashboard,
                Screen::Visualization,
                Screen::Devices,
                Screen::Profiles,
                Screen::Calibration,
                Screen::PythonLab,
                Screen::Settings,
            ],
        }
    }
}
