//! # Action Preview Widget
//!
//! Displays a virtual cursor minimap, direction indicator, action log,
//! and real-time feedback about decoded HID actions.

use std::collections::VecDeque;
use eframe::egui;
use crate::widgets::{Widget, WidgetContext, WidgetId};

/// Maximum action log entries to retain.
const MAX_LOG_ENTRIES: usize = 50;

#[derive(Clone)]
struct ActionLogEntry {
    /// Wall clock time string.
    time_str: String,
    /// Short description of the action.
    description: String,
    /// Confidence of the action.
    confidence: f32,
    /// Color for the entry.
    color: egui::Color32,
}

pub struct ActionPreviewWidget {
    /// Virtual cursor position (normalized 0–1).
    cursor_x: f32,
    cursor_y: f32,
    /// Cursor trail for visualizing movement.
    cursor_trail: VecDeque<(f32, f32)>,
    /// Action log.
    log: VecDeque<ActionLogEntry>,
    /// Whether to show the cursor trail.
    show_trail: bool,
    /// Accumulated actions count for stats.
    total_actions: u64,
}

impl ActionPreviewWidget {
    pub fn new() -> Self {
        Self {
            cursor_x: 0.5,
            cursor_y: 0.5,
            cursor_trail: VecDeque::new(),
            log: VecDeque::new(),
            show_trail: true,
            total_actions: 0,
        }
    }

    fn process_actions(&mut self, ctx: &WidgetContext<'_>) {
        for action in ctx.bus.actions.iter() {
            if action.is_none() {
                continue;
            }

            self.total_actions += 1;

            // Process mouse movements
            if let Some(ref mouse) = action.mouse {
                if let Some(ref mv) = mouse.movement {
                    // Apply movement (scaled down for the minimap)
                    self.cursor_x = (self.cursor_x + mv.dx * 0.01).clamp(0.0, 1.0);
                    self.cursor_y = (self.cursor_y + mv.dy * 0.01).clamp(0.0, 1.0);

                    self.cursor_trail.push_back((self.cursor_x, self.cursor_y));
                    while self.cursor_trail.len() > 200 {
                        self.cursor_trail.pop_front();
                    }

                    self.add_log(
                        format!("Move ({:+.1}, {:+.1})", mv.dx, mv.dy),
                        action.confidence,
                        egui::Color32::from_rgb(100, 181, 246),
                    );
                }

                // Process clicks
                for btn in &mouse.buttons {
                    self.add_log(
                        format!("{:?} {}", btn.button, if btn.pressed { "pressed" } else { "released" }),
                        action.confidence,
                        egui::Color32::from_rgb(129, 199, 132),
                    );
                }

                // Process scroll
                if let Some(ref scroll) = mouse.scroll {
                    self.add_log(
                        format!("Scroll ({:+.0}, {:+.0})", scroll.dx, scroll.dy),
                        action.confidence,
                        egui::Color32::from_rgb(255, 213, 79),
                    );
                }
            }

            // Process keyboard actions
            if let Some(ref key) = action.keyboard {
                self.add_log(
                    format!("Key: {:?}", key),
                    action.confidence,
                    egui::Color32::from_rgb(206, 147, 216),
                );
            }
        }
    }

    fn add_log(&mut self, description: String, confidence: f32, color: egui::Color32) {
        let now = chrono::Local::now();
        self.log.push_back(ActionLogEntry {
            time_str: now.format("%H:%M:%S%.3f").to_string(),
            description,
            confidence,
            color,
        });
        while self.log.len() > MAX_LOG_ENTRIES {
            self.log.pop_front();
        }
    }
}

impl Widget for ActionPreviewWidget {
    fn id(&self) -> WidgetId {
        WidgetId::ActionPreview
    }

    fn title(&self) -> &str {
        "Action Preview"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>) {
        self.process_actions(ctx);

        // Controls
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_trail, "Trail");
            if ui.button("Reset Cursor").clicked() {
                self.cursor_x = 0.5;
                self.cursor_y = 0.5;
                self.cursor_trail.clear();
            }
            if ui.button("Clear Log").clicked() {
                self.log.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("Total: {}", self.total_actions)).small().weak());
            });
        });

        // Split area: minimap on left, log on right
        let available = ui.available_size();
        let minimap_size = available.y.min(available.x * 0.4).min(200.0);

        ui.horizontal(|ui| {
            // === Cursor minimap ===
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Virtual Cursor").small().strong());
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(minimap_size, minimap_size),
                    egui::Sense::hover(),
                );

                if ui.is_rect_visible(rect) {
                    let painter = ui.painter_at(rect);

                    // Background
                    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(20));
                    painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

                    // Grid
                    for i in 1..4 {
                        let frac = i as f32 / 4.0;
                        let x = rect.left() + frac * rect.width();
                        let y = rect.top() + frac * rect.height();
                        painter.line_segment(
                            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                            egui::Stroke::new(0.3, egui::Color32::from_gray(40)),
                        );
                        painter.line_segment(
                            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                            egui::Stroke::new(0.3, egui::Color32::from_gray(40)),
                        );
                    }

                    // Trail
                    if self.show_trail && self.cursor_trail.len() >= 2 {
                        let trail_points: Vec<egui::Pos2> = self.cursor_trail.iter()
                            .enumerate()
                            .map(|(i, (x, y))| {
                                let _ = i; // opacity fade could use this
                                egui::pos2(
                                    rect.left() + x * rect.width(),
                                    rect.top() + y * rect.height(),
                                )
                            })
                            .collect();

                        painter.add(egui::Shape::line(
                            trail_points,
                            egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(100, 181, 246, 80)),
                        ));
                    }

                    // Cursor crosshair
                    let cx = rect.left() + self.cursor_x * rect.width();
                    let cy = rect.top() + self.cursor_y * rect.height();
                    let cross_size = 8.0;

                    painter.line_segment(
                        [egui::pos2(cx - cross_size, cy), egui::pos2(cx + cross_size, cy)],
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 100, 100)),
                    );
                    painter.line_segment(
                        [egui::pos2(cx, cy - cross_size), egui::pos2(cx, cy + cross_size)],
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 100, 100)),
                    );
                    painter.circle_filled(
                        egui::pos2(cx, cy),
                        3.0,
                        egui::Color32::from_rgb(255, 100, 100),
                    );

                    // Coordinate label
                    painter.text(
                        egui::pos2(rect.left() + 4.0, rect.bottom() - 2.0),
                        egui::Align2::LEFT_BOTTOM,
                        format!("({:.2}, {:.2})", self.cursor_x, self.cursor_y),
                        egui::FontId::proportional(9.0),
                        egui::Color32::from_gray(120),
                    );
                }
            });

            // === Action log ===
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Action Log").small().strong());

                egui::ScrollArea::vertical()
                    .max_height(minimap_size)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.log.is_empty() {
                            ui.label(egui::RichText::new("No actions yet...").weak());
                        }

                        for entry in self.log.iter().rev().take(30) {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&entry.time_str)
                                        .monospace()
                                        .small()
                                        .color(egui::Color32::from_gray(100)),
                                );
                                ui.label(
                                    egui::RichText::new(&entry.description)
                                        .color(entry.color)
                                        .small(),
                                );
                                // Confidence badge
                                let conf_color = if entry.confidence > 0.7 {
                                    egui::Color32::from_rgb(76, 175, 80)
                                } else if entry.confidence > 0.4 {
                                    egui::Color32::from_rgb(255, 193, 7)
                                } else {
                                    egui::Color32::from_rgb(244, 67, 54)
                                };
                                ui.label(
                                    egui::RichText::new(format!("{:.0}%", entry.confidence * 100.0))
                                        .small()
                                        .color(conf_color),
                                );
                            });
                        }
                    });
            });
        });
    }
}
