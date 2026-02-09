//! # Visualization Screen
//!
//! The main real-time data visualization screen. Uses the LayoutManager to
//! display multiple widgets (Time Series, FFT, Band Power, etc.) in a
//! configurable multi-pane layout.

use eframe::egui;

use crate::data_bus::DataBus;
use crate::layout::LayoutManager;
use crate::state::ServiceSnapshot;
use crate::widgets::WidgetContext;

/// The visualization screen manages the layout and renders all active widgets.
pub struct VisualizationScreen {
    layout: LayoutManager,
}

impl VisualizationScreen {
    pub fn new() -> Self {
        Self {
            layout: LayoutManager::new(),
        }
    }

    /// Render the visualization screen.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        bus: &DataBus,
        snapshot: &ServiceSnapshot,
    ) {
        let ctx = WidgetContext { bus, snapshot };

        // Layout toolbar (layout selector) + connection status
        ui.horizontal(|ui| {
            self.layout.show_toolbar(ui);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !snapshot.running {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 152, 0),
                        "⚠ Service stopped",
                    );
                } else if bus.samples.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_gray(120),
                        "⏳ Waiting for data…",
                    );
                } else {
                    ui.colored_label(
                        egui::Color32::from_rgb(76, 175, 80),
                        format!("● {} samples", bus.samples.len()),
                    );
                }
            });
        });

        ui.separator();

        // Always render the pane layout — individual widgets handle empty data
        self.layout.show_panes(ui, &ctx);
    }
}
