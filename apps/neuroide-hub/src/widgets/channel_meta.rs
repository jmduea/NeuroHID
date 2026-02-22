use eframe::egui;

pub const EEG_CHANNEL_NAMES: &[&str] = &["AF3", "AF4", "T7", "T8", "Pz"];

pub const EEG_CHANNEL_COLORS: &[egui::Color32] = &[
    egui::Color32::from_rgb(129, 199, 132),
    egui::Color32::from_rgb(100, 181, 246),
    egui::Color32::from_rgb(239, 154, 154),
    egui::Color32::from_rgb(255, 213, 79),
    egui::Color32::from_rgb(206, 147, 216),
    egui::Color32::from_rgb(255, 183, 77),
    egui::Color32::from_rgb(128, 222, 234),
    egui::Color32::from_rgb(240, 98, 146),
];

pub const EEG_HEAD_POSITIONS: &[(f32, f32)] = &[
    (0.35, 0.25),
    (0.65, 0.25),
    (0.15, 0.50),
    (0.85, 0.50),
    (0.50, 0.70),
];
