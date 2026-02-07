//! # Hub Screens
//!
//! Each screen renders into the hub's central panel area. The active screen
//! is determined by sidebar navigation in `app.rs`.

pub mod dashboard;
pub mod devices;
pub mod profiles;
pub mod calibration;
pub mod settings;

/// The available hub screens, selected via sidebar navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    Devices,
    Profiles,
    Calibration,
    Settings,
}

impl Screen {
    pub fn label(&self) -> &'static str {
        match self {
            Screen::Dashboard => "Dashboard",
            Screen::Devices => "Devices",
            Screen::Profiles => "Profiles",
            Screen::Calibration => "Calibration",
            Screen::Settings => "Settings",
        }
    }

    pub fn all() -> &'static [Screen] {
        &[
            Screen::Dashboard,
            Screen::Devices,
            Screen::Profiles,
            Screen::Calibration,
            Screen::Settings,
        ]
    }
}
