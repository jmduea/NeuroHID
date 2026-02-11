//! # Layout Engine
//!
//! Manages the multi-pane widget layout for the Visualization screen.
//! Users choose a layout configuration (e.g., 2x2, 1+2) and assign
//! a widget to each pane via a dropdown. Supports drag-and-drop to
//! swap widgets between panes.

use crate::widgets::{create_widget, Widget, WidgetContext, WidgetId};
use eframe::egui::{self, Color32, CursorIcon, Margin, Rounding, Stroke};

/// Available layout configurations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LayoutConfig {
    /// Single full-width pane.
    Single,
    /// Two panes side by side.
    TwoColumns,
    /// Two panes stacked vertically.
    TwoRows,
    /// 2x2 grid.
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
            LayoutConfig::Grid2x2 => "2x2 Grid",
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

/// Result of rendering a single pane, used for drag-drop state tracking.
struct PaneRenderResult {
    _is_hovered: bool,
}

/// Color constants for drag-and-drop visual feedback.
mod colors {
    use super::Color32;

    /// Material blue for active drag highlighting.
    pub const DRAG_HIGHLIGHT: Color32 = Color32::from_rgb(66, 165, 245);
    /// Subtle green for valid drop target.
    pub const DROP_TARGET: Color32 = Color32::from_rgb(76, 175, 80);
    /// Dark pane background.
    pub const PANE_BG: Color32 = Color32::from_gray(25);
    /// Subtle pane border.
    pub const PANE_BORDER: Color32 = Color32::from_gray(50);
    /// Header bar background.
    pub const HEADER_BG: Color32 = Color32::from_gray(35);
    /// Drag handle color (normal).
    pub const DRAG_HANDLE: Color32 = Color32::from_gray(120);
    /// Drag handle color (hovered).
    pub const DRAG_HANDLE_HOVER: Color32 = Color32::from_gray(180);
}

/// The layout manager for the Visualization workspace.
pub struct LayoutManager {
    pub config: LayoutConfig,
    panes: Vec<Pane>,
    /// Index of the pane currently being dragged, if any.
    drag_source: Option<usize>,
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

        Self {
            config,
            panes,
            drag_source: None,
        }
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

        // Clear any active drag when layout changes
        self.drag_source = None;

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
    #[allow(dead_code)]
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
                        if ui
                            .selectable_value(&mut self.config, layout, layout.label())
                            .changed()
                        {
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
                let top_down = egui::Layout::top_down(egui::Align::Min);
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(available.x * 0.5 - 4.0, available.y),
                        top_down,
                        |ui| {
                            self.show_single_pane(ui, 0, ctx);
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(available.x * 0.5 - 4.0, available.y),
                        top_down,
                        |ui| {
                            self.show_single_pane(ui, 1, ctx);
                        },
                    );
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
                let top_down = egui::Layout::top_down(egui::Align::Min);

                ui.allocate_ui(egui::vec2(available.x, half_h), |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(egui::vec2(half_w, half_h), top_down, |ui| {
                            self.show_single_pane(ui, 0, ctx);
                        });
                        ui.allocate_ui_with_layout(egui::vec2(half_w, half_h), top_down, |ui| {
                            self.show_single_pane(ui, 1, ctx);
                        });
                    });
                });
                ui.allocate_ui(egui::vec2(available.x, half_h), |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(egui::vec2(half_w, half_h), top_down, |ui| {
                            self.show_single_pane(ui, 2, ctx);
                        });
                        ui.allocate_ui_with_layout(egui::vec2(half_w, half_h), top_down, |ui| {
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
                let top_down = egui::Layout::top_down(egui::Align::Min);

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(left_w, available.y), top_down, |ui| {
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
                let top_down = egui::Layout::top_down(egui::Align::Min);

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.allocate_ui(egui::vec2(left_w, half_h), |ui| {
                            self.show_single_pane(ui, 0, ctx);
                        });
                        ui.allocate_ui(egui::vec2(left_w, half_h), |ui| {
                            self.show_single_pane(ui, 1, ctx);
                        });
                    });
                    ui.allocate_ui_with_layout(egui::vec2(right_w, available.y), top_down, |ui| {
                        self.show_single_pane(ui, 2, ctx);
                    });
                });
            }
        }

        // Clear drag state if pointer released anywhere
        let pointer_released = ui.input(|i| i.pointer.any_released());
        if pointer_released && self.drag_source.is_some() {
            self.drag_source = None;
        }
    }

    /// Render a single pane: drag handle + widget selector dropdown + widget content.
    fn show_single_pane(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        ctx: &WidgetContext<'_>,
    ) -> PaneRenderResult {
        let is_drag_source = self.drag_source == Some(index);
        let is_dragging = self.drag_source.is_some();
        let is_potential_drop_target = is_dragging && !is_drag_source;

        // Determine border style based on drag state
        let border_stroke = if is_drag_source {
            Stroke::new(2.0, colors::DRAG_HIGHLIGHT)
        } else if is_potential_drop_target {
            Stroke::new(1.5, colors::DROP_TARGET.gamma_multiply(0.6))
        } else {
            Stroke::new(1.0, colors::PANE_BORDER)
        };

        // Custom frame for the pane
        let pane_frame = egui::Frame::none()
            .fill(colors::PANE_BG)
            .stroke(border_stroke)
            .rounding(Rounding::same(6.0))
            .inner_margin(Margin::same(4.0));

        let frame_response = pane_frame.show(ui, |ui| {
            ui.set_min_size(ui.available_size());

            // Header bar with drag handle and widget selector
            let header_frame = egui::Frame::none()
                .fill(colors::HEADER_BG)
                .rounding(Rounding {
                    nw: 4.0,
                    ne: 4.0,
                    sw: 0.0,
                    se: 0.0,
                })
                .inner_margin(Margin::symmetric(6.0, 4.0));

            header_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Drag handle
                    let handle_response = self.show_drag_handle(ui, index);

                    // Handle drag start
                    if handle_response.drag_started() {
                        self.drag_source = Some(index);
                    }

                    ui.add_space(4.0);

                    // Widget selector dropdown - need to get pane data
                    if let Some(pane) = self.panes.get_mut(index) {
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
                    }

                    // Spacer to push any additional controls to the right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Optional: pane index indicator (subtle)
                        ui.label(
                            egui::RichText::new(format!("#{}", index + 1))
                                .small()
                                .color(Color32::from_gray(80)),
                        );
                    });
                });
            });

            // Check if this pane is hovered (for drop target detection)
            let pane_rect = ui.available_rect_before_wrap();
            let is_hovered = ui.rect_contains_pointer(pane_rect);

            // Show drop indicator if this is a potential drop target and hovered
            if is_potential_drop_target && is_hovered {
                // Draw a more prominent drop indicator
                ui.painter().rect_stroke(
                    pane_rect.shrink(2.0),
                    Rounding::same(4.0),
                    Stroke::new(2.0, colors::DROP_TARGET),
                );

                // "Drop here" text overlay
                let center = pane_rect.center();
                ui.painter().text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    "Drop to swap",
                    egui::FontId::proportional(14.0),
                    colors::DROP_TARGET,
                );

                // Handle the drop
                let pointer_released = ui.input(|i| i.pointer.any_released());
                if pointer_released {
                    if let Some(source) = self.drag_source {
                        if source != index {
                            // Perform the swap
                            self.panes.swap(source, index);
                        }
                        self.drag_source = None;
                    }
                }
            }

            ui.add_space(4.0);

            // Widget content area
            if ui.available_size().y < 40.0 {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("Panel too small")
                            .small()
                            .color(Color32::from_gray(100)),
                    );
                });
            } else if let Some(pane) = self.panes.get_mut(index) {
                pane.widget.show(ui, ctx);
            }
        });

        // Check if this pane is hovered (for drop target detection)
        let is_hovered = frame_response
            .response
            .rect
            .contains(ui.ctx().pointer_hover_pos().unwrap_or(egui::Pos2::ZERO));

        PaneRenderResult {
            _is_hovered: is_hovered,
        }
    }

    /// Render the drag handle and return its response for drag detection.
    fn show_drag_handle(&self, ui: &mut egui::Ui, _index: usize) -> egui::Response {
        // Allocate space for the drag handle
        let (rect, response) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::drag());

        // Determine handle color based on state
        let handle_color = if response.dragged() {
            colors::DRAG_HIGHLIGHT
        } else if response.hovered() {
            colors::DRAG_HANDLE_HOVER
        } else {
            colors::DRAG_HANDLE
        };

        // Draw the grip icon (6-dot pattern)
        let painter = ui.painter();
        let center = rect.center();
        let dot_radius = 1.5;
        let spacing = 4.0;

        // Draw 6 dots in a 2x3 pattern
        for row in 0..3 {
            for col in 0..2 {
                let x = center.x + (col as f32 - 0.5) * spacing;
                let y = center.y + (row as f32 - 1.0) * spacing;
                painter.circle_filled(egui::pos2(x, y), dot_radius, handle_color);
            }
        }

        // Set cursor based on state
        if response.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }

        response
    }
}
