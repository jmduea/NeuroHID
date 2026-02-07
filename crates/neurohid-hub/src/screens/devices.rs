//! # Devices Screen
//!
//! Device discovery and connection management. Shows discovered devices,
//! connection status, and per-channel signal quality.

use eframe::egui;

use crate::state::HubState;

pub struct DevicesScreen;

impl DevicesScreen {
    pub fn new() -> Self {
        Self
    }

    pub fn show(&mut self, ui: &mut egui::Ui, state: &HubState) {
        ui.heading("Devices");
        ui.add_space(16.0);

        let snap = &state.service_snapshot;

        if !snap.running {
            ui.label("Start the service to discover and connect to EEG devices.");
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Go to Dashboard to start the service.")
                    .color(egui::Color32::GRAY),
            );
            return;
        }

        // Connected device detail
        if snap.device_connected {
            ui.group(|ui| {
                ui.heading("Connected Device");
                ui.add_space(8.0);

                if let Some(name) = &snap.device_name {
                    ui.label(format!("Name: {}", name));
                }
                ui.label("Type: Mock Device (MVP)");
                ui.label("Channels: 5 (AF3, AF4, T7, T8, Pz)");
                ui.label("Sample Rate: 128 Hz");

                if let Some(battery) = snap.device_battery {
                    ui.add_space(4.0);
                    ui.label(format!("Battery: {}%", battery));
                }

                ui.add_space(12.0);
                ui.heading("Signal Quality");
                ui.add_space(4.0);

                // Overall quality bar
                let quality = snap.signal_quality;
                let quality_color = if quality > 0.7 {
                    egui::Color32::GREEN
                } else if quality > 0.5 {
                    egui::Color32::YELLOW
                } else {
                    egui::Color32::RED
                };

                ui.add(
                    egui::ProgressBar::new(quality)
                        .text(format!("Overall: {:.0}%", quality * 100.0))
                        .fill(quality_color),
                );

                // Per-channel quality bars (simulated for MVP)
                ui.add_space(8.0);
                let channels = ["AF3", "AF4", "T7", "T8", "Pz"];
                for (i, name) in channels.iter().enumerate() {
                    // Simulate per-channel variation around the overall quality
                    let channel_q = (quality + (i as f32 * 0.03 - 0.06)).clamp(0.0, 1.0);
                    ui.horizontal(|ui| {
                        ui.label(format!("{:>3}:", name));
                        ui.add(
                            egui::ProgressBar::new(channel_q)
                                .text(format!("{:.0}%", channel_q * 100.0))
                                .desired_width(200.0),
                        );
                    });
                }
            });
        } else {
            ui.group(|ui| {
                ui.heading("No Device Connected");
                ui.add_space(8.0);
                ui.label("The service is running but no device is connected.");
                ui.label("The mock device should connect automatically.");
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Check service logs for connection errors.")
                        .small()
                        .color(egui::Color32::YELLOW),
                );
            });
        }
    }
}
