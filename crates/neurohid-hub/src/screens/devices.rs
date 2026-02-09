//! # Devices Screen
//!
//! Stream discovery and connection management. Shows discovered LSL streams,
//! connection status, and per-stream signal quality.

use eframe::egui;

use crate::service_manager::ServiceManager;
use crate::state::HubState;

pub struct DevicesScreen;

impl DevicesScreen {
    pub fn new() -> Self {
        Self
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &HubState,
        service_manager: &mut ServiceManager,
    ) {
        ui.heading("Devices");
        ui.add_space(16.0);

        let snap = &state.service_snapshot;

        if !snap.running {
            ui.label("Start the service to discover and connect to LSL streams.");
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Go to Dashboard to start the service.")
                    .color(egui::Color32::GRAY),
            );
            return;
        }

        // Header with rescan button
        ui.horizontal(|ui| {
            ui.heading("Available Streams");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Rescan").clicked() {
                    service_manager.rescan_streams();
                }
            });
        });
        ui.add_space(8.0);

        if snap.discovered_streams.is_empty() {
            ui.group(|ui| {
                ui.label("No streams found.");
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Ensure your device software is running and pushing to LSL.\n\
                         Streams are rescanned automatically every 10 seconds.",
                    )
                    .small()
                    .color(egui::Color32::YELLOW),
                );
            });
        } else {
            // Stream list
            for stream in &snap.discovered_streams {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        // Status indicator
                        let (color, status) = if stream.connected {
                            (egui::Color32::GREEN, "Connected")
                        } else {
                            (egui::Color32::GRAY, "Available")
                        };
                        ui.colored_label(color, "\u{25CF}"); // bullet

                        // Stream info
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&stream.name).strong());
                                // Battery indicator next to name
                                if let Some(bat) = stream.battery_percent {
                                    let bat_color = if bat > 50 {
                                        egui::Color32::GREEN
                                    } else if bat > 20 {
                                        egui::Color32::YELLOW
                                    } else {
                                        egui::Color32::RED
                                    };
                                    ui.colored_label(bat_color, format!("{}%", bat));
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&stream.stream_type)
                                        .small()
                                        .color(egui::Color32::LIGHT_GRAY),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{}ch @ {:.0} Hz",
                                        stream.channel_count, stream.sample_rate
                                    ))
                                    .small()
                                    .color(egui::Color32::LIGHT_GRAY),
                                );
                                ui.label(
                                    egui::RichText::new(status).small().color(color),
                                );
                            });
                        });

                        // Connect/Disconnect button (right-aligned)
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if stream.connected {
                                    if ui.button("Disconnect").clicked() {
                                        service_manager.disconnect_stream(&stream.id);
                                    }
                                } else if ui.button("Connect").clicked() {
                                    service_manager.connect_stream(&stream.id);
                                }
                            },
                        );
                    });

                    // Per-channel quality bars for connected streams
                    if stream.connected {
                        if let Some(qualities) = &stream.channel_quality {
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("Channel Quality")
                                    .small()
                                    .color(egui::Color32::GRAY),
                            );
                            for (i, &q) in qualities.iter().enumerate() {
                                let q_color = if q > 0.7 {
                                    egui::Color32::GREEN
                                } else if q > 0.4 {
                                    egui::Color32::YELLOW
                                } else {
                                    egui::Color32::RED
                                };
                                ui.add(
                                    egui::ProgressBar::new(q)
                                        .text(format!("Ch{}: {:.0}%", i, q * 100.0))
                                        .fill(q_color)
                                        .desired_width(ui.available_width()),
                                );
                            }
                        }
                    }
                });
                ui.add_space(4.0);
            }
        }

        // Connected stream detail section
        let connected_count = snap
            .discovered_streams
            .iter()
            .filter(|s| s.connected)
            .count();

        if connected_count > 0 {
            ui.add_space(12.0);
            ui.heading("Signal Quality");
            ui.add_space(8.0);

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

            ui.add_space(4.0);
            let stream_names: Vec<&str> = snap
                .discovered_streams
                .iter()
                .filter(|s| s.connected)
                .map(|s| s.name.as_str())
                .collect();
            ui.label(
                egui::RichText::new(format!("Aggregate across: {}", stream_names.join(", ")))
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }
    }
}
