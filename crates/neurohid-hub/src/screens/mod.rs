//! # Hub Screens
//!
//! Each screen renders into the hub's central panel area. The active screen
//! is determined by sidebar navigation in `app.rs`.

use neurohid_types::config::UiMode;

pub mod calibration;
pub mod dashboard;
pub mod devices;
pub mod jupyter_ide;
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
    Training,
    JupyterIde,
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
            Screen::Training => "Training",
            Screen::JupyterIde => "Jupyter IDE",
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
                Screen::Training,
                Screen::Settings,
            ],
            UiMode::Advanced => &[
                Screen::Dashboard,
                Screen::Visualization,
                Screen::Devices,
                Screen::Profiles,
                Screen::Calibration,
                Screen::Training,
                Screen::JupyterIde,
                Screen::PythonLab,
                Screen::Settings,
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use neurohid_types::config::UiMode;

    use super::Screen;

    #[test]
    fn standard_mode_hides_advanced_only_screens() {
        let standard = Screen::all_for_mode(&UiMode::Standard);
        assert!(!standard.contains(&Screen::Visualization));
        assert!(!standard.contains(&Screen::JupyterIde));
        assert!(!standard.contains(&Screen::PythonLab));
        assert!(standard.contains(&Screen::Dashboard));
        assert!(standard.contains(&Screen::Settings));
    }

    #[test]
    fn advanced_mode_contains_all_standard_screens_plus_extras() {
        let standard = Screen::all_for_mode(&UiMode::Standard);
        let advanced = Screen::all_for_mode(&UiMode::Advanced);

        for screen in standard {
            assert!(advanced.contains(screen));
        }
        assert!(advanced.contains(&Screen::Visualization));
        assert!(advanced.contains(&Screen::JupyterIde));
        assert!(advanced.contains(&Screen::PythonLab));
    }
}
