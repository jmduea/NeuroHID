//! # Layout Engine
//!
//! Manages the multi-pane widget layout for the Visualization screen.
//! Users choose a layout configuration (e.g., 2×2, 1+2) and assign
//! a widget to each pane via a dropdown.

use crate::widgets::{Widget, WidgetId, WidgetContext, create_widget};
use eframe::egui;

/// Available layout configurations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LayoutConfig {
    /// Single full-width pane.
    Single,
    /// Two panes side by side.
    TwoColumns,
    /// Two panes stacked vertically.
    TwoRows,
    /// 2×2 grid.
    Grid2x2,
    /// One large pane on the left, two smaller on the right.
    OneLeftTwoRight,
    /// Two smaller on the left, one large on the right.
    TwoLeftOneRight,
}

impl LayoutConfig {
    pub const ALL: &'static [LayoutConfig] = &[
        LayoutConfig::Single,
        LayoutConfig::TwoColumns,
        LayoutConfig::TwoRows,
        LayoutConfig::Grid2x2,
        LayoutConfig::OneLeftTwoRight,
        LayoutConfig::TwoLeftOneRight,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            LayoutConfig::Single => "1 Pane",
            LayoutConfig::TwoColumns => "2 Columns",
            LayoutConfig::TwoRows => "2 Rows",
            LayoutConfig::Grid2x2 => "2×2 Grid",
            LayoutConfig::OneLeftTwoRight => "1 + 2",
            LayoutConfig::TwoLeftOneRight => "2 + 1",
        }
    }

    /// Number of panes in this layout.
    pub fn pane_count(&self) -> usize {
        match self {
            LayoutConfig::Single => 1,
            LayoutConfig::TwoColumns | LayoutConfig::TwoRows => 2,
            LayoutConfig::Grid2x2 => 4,
            LayoutConfig::OneLeftTwoRight | LayoutConfig::TwoLeftOneRight => 3,
        }
    }
}

/// A single pane in the layout — holds the selected widget ID and widget instance.
struct Pane {
    widget_id: WidgetId,
    widget: Box<dyn Widget>,
}

/// The layout manager for the Visualization workspace.
pub struct LayoutManager {
    pub config: LayoutConfig,
    panes: Vec<Pane>,
}

impl LayoutManager {
    /// Create a new layout manager with default widget assignments.
    pub fn new() -> Self {
        let defaults = [
            WidgetId::TimeSeries,
            WidgetId::FftPlot,
            WidgetId::BandPower,
            WidgetId::SignalQuality,
        ];

        let config = LayoutConfig::Grid2x2;
        let panes = (0..config.pane_count())
            .map(|i| {
                let id = defaults[i % defaults.len()];
                Pane {
                    widget_id: id,
                    widget: create_widget(id),
                }
            })
            .collect();

        Self { config, panes }
    }

    /// Change the layout configuration, keeping as many pane assignments
    /// as possible and filling new panes with defaults.
    pub fn set_layout(&mut self, config: LayoutConfig) {
        let new_count = config.pane_count();
        let defaults = [
            WidgetId::TimeSeries,
            WidgetId::FftPlot,
            WidgetId::BandPower,
            WidgetId::SignalQuality,
        ];

        // Grow panes if needed
        while self.panes.len() < new_count {
            let idx = self.panes.len();
            let id = defaults[idx % defaults.len()];
            self.panes.push(Pane {
                widget_id: id,
                widget: create_widget(id),
            });
        }
        // Shrink if needed
        self.panes.truncate(new_count);
        self.config = config;
    }

    /// Change the widget in a specific pane.
    pub fn set_pane_widget(&mut self, pane_index: usize, widget_id: WidgetId) {
        if let Some(pane) = self.panes.get_mut(pane_index) {
            if pane.widget_id != widget_id {
                pane.widget_id = widget_id;
                pane.widget = create_widget(widget_id);
            }
        }
    }

    /// Render the layout toolbar (layout selector).
    pub fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Layout:");
            let current_label = self.config.label();
            egui::ComboBox::from_id_source("layout_selector")
                .selected_text(current_label)
                .show_ui(ui, |ui: &mut egui::Ui| {
                    for &layout in LayoutConfig::ALL {
                        if ui.selectable_value(
                            &mut self.config,
                            layout,
                            layout.label(),
                        ).changed() {
                            let new_config = self.config;
                            self.set_layout(new_config);
                        }
                    }
                });
        });
    }

    /// Render all panes with their widgets.
    pub fn show_panes(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>) {
        match self.config {
            LayoutConfig::Single => {
                self.show_single_pane(ui, 0, ctx);
            }
            LayoutConfig::TwoColumns => {
                let available = ui.available_size();
                ui.horizontal(|ui| {
                    ui.allocate_ui(egui::vec2(available.x * 0.5 - 4.0, available.y), |ui| {
                        self.show_single_pane(ui, 0, ctx);
                    });
                    ui.allocate_ui(egui::vec2(available.x * 0.5 - 4.0, available.y), |ui| {
                        self.show_single_pane(ui, 1, ctx);
                    });
                });
            }
            LayoutConfig::TwoRows => {
                let available = ui.available_size();
                ui.allocate_ui(egui::vec2(available.x, available.y * 0.5 - 4.0), |ui| {
                    self.show_single_pane(ui, 0, ctx);
                });
                ui.allocate_ui(egui::vec2(available.x, available.y * 0.5 - 4.0), |ui| {
                    self.show_single_pane(ui, 1, ctx);
                });
            }
            LayoutConfig::Grid2x2 => {
                let available = ui.available_size();
                let half_w = available.x * 0.5 - 4.0;
                let half_h = available.y * 0.5 - 4.0;

                ui.allocate_ui(egui::vec2(available.x, half_h), |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(egui::vec2(half_w, half_h), |ui| {
                            self.show_single_pane(ui, 0, ctx);
                        });
                        ui.allocate_ui(egui::vec2(half_w, half_h), |ui| {
                            self.show_single_pane(ui, 1, ctx);
                        });
                    });
                });
                ui.allocate_ui(egui::vec2(available.x, half_h), |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(egui::vec2(half_w, half_h), |ui| {
                            self.show_single_pane(ui, 2, ctx);
                        });
                        ui.allocate_ui(egui::vec2(half_w, half_h), |ui| {
                            self.show_single_pane(ui, 3, ctx);
                        });
                    });
                });
            }
            LayoutConfig::OneLeftTwoRight => {
                let available = ui.available_size();
                let left_w = available.x * 0.5 - 4.0;
                let right_w = available.x * 0.5 - 4.0;
                let half_h = available.y * 0.5 - 4.0;

                ui.horizontal(|ui| {
                    ui.allocate_ui(egui::vec2(left_w, available.y), |ui| {
                        self.show_single_pane(ui, 0, ctx);
                    });
                    ui.vertical(|ui| {
                        ui.allocate_ui(egui::vec2(right_w, half_h), |ui| {
                            self.show_single_pane(ui, 1, ctx);
                        });
                        ui.allocate_ui(egui::vec2(right_w, half_h), |ui| {
                            self.show_single_pane(ui, 2, ctx);
                        });
                    });
                });
            }
            LayoutConfig::TwoLeftOneRight => {
                let available = ui.available_size();
                let left_w = available.x * 0.5 - 4.0;
                let right_w = available.x * 0.5 - 4.0;
                let half_h = available.y * 0.5 - 4.0;

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.allocate_ui(egui::vec2(left_w, half_h), |ui| {
                            self.show_single_pane(ui, 0, ctx);
                        });
                        ui.allocate_ui(egui::vec2(left_w, half_h), |ui| {
                            self.show_single_pane(ui, 1, ctx);
                        });
                    });
                    ui.allocate_ui(egui::vec2(right_w, available.y), |ui| {
                        self.show_single_pane(ui, 2, ctx);
                    });
                });
            }
        }
    }

    /// Render a single pane: widget selector dropdown + widget content.
    fn show_single_pane(&mut self, ui: &mut egui::Ui, index: usize, ctx: &WidgetContext<'_>) {
        let Some(pane) = self.panes.get_mut(index) else { return };

        egui::Frame::group(ui.style())
            .show(ui, |ui| {
                // Widget selector dropdown at the top of each pane
                ui.horizontal(|ui| {
                    let current_label = pane.widget_id.label();
                    let combo_id = format!("pane_widget_{}", index);
                    let mut new_id = pane.widget_id;
                    egui::ComboBox::from_id_source(combo_id)
                        .selected_text(current_label)
                        .width(140.0)
                        .show_ui(ui, |ui: &mut egui::Ui| {
                            for &wid in WidgetId::ALL {
                                ui.selectable_value(&mut new_id, wid, wid.label());
                            }
                        });
                    if new_id != pane.widget_id {
                        pane.widget_id = new_id;
                        pane.widget = create_widget(new_id);
                    }
                });

                ui.separator();

                // Widget content
                pane.widget.show(ui, ctx);
            });
    }
}
