//! # Action Preview Widget
//!
//! Displays a virtual cursor minimap, direction indicator, action log,
//! and real-time feedback about decoded HID actions.

use std::collections::VecDeque;
use eframe::egui;
use crate::widgets::{Widget, WidgetContext, WidgetId};

/// Maximum action log entries to retain.
const MAX_LOG_ENTRIES: usize = 50;

/// Action filter options for the log.
#[derive(Clone, Copy, PartialEq, Default)]
enum ActionFilter {
    #[default]
    All,
    Mouse,
    Keyboard,
    Scroll,
}

impl ActionFilter {
    fn label(&self) -> &'static str {
        match self {
            ActionFilter::All => "All",
            ActionFilter::Mouse => "Mouse",
            ActionFilter::Keyboard => "Keyboard",
            ActionFilter::Scroll => "Scroll",
        }
    }

    fn matches(&self, entry: &ActionLogEntry) -> bool {
        match self {
            ActionFilter::All => true,
            ActionFilter::Mouse => entry.action_type == ActionType::Move || entry.action_type == ActionType::Click,
            ActionFilter::Keyboard => entry.action_type == ActionType::Key,
            ActionFilter::Scroll => entry.action_type == ActionType::Scroll,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ActionType {
    Move,
    Click,
    Scroll,
    Key,
}

#[derive(Clone)]
struct ActionLogEntry {
    /// Wall clock time string.
    time_str: String,
    /// Timestamp for relative time display.
    timestamp: std::time::Instant,
    /// Short description of the action.
    description: String,
    /// Confidence of the action.
    confidence: f32,
    /// Color for the entry.
    color: egui::Color32,
    /// Action type for filtering.
    action_type: ActionType,
    /// Count for grouped entries.
    count: u32,
}

pub struct ActionPreviewWidget {
    /// Virtual cursor position (normalized 0-1).
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
    /// Filter for action log.
    filter: ActionFilter,
    /// Total distance traveled.
    total_distance: f32,
    /// Last cursor position for distance calculation.
    _last_cursor_pos: Option<(f32, f32)>,
    /// Session start time for speed calculation.
    session_start: std::time::Instant,
    /// Show relative timestamps.
    show_relative_time: bool,
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
            filter: ActionFilter::All,
            total_distance: 0.0,
            _last_cursor_pos: None,
            session_start: std::time::Instant::now(),
            show_relative_time: true,
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
                    // Store previous position
                    let prev_x = self.cursor_x;
                    let prev_y = self.cursor_y;

                    // Apply movement (scaled down for the minimap)
                    self.cursor_x = (self.cursor_x + mv.dx * 0.01).clamp(0.0, 1.0);
                    self.cursor_y = (self.cursor_y + mv.dy * 0.01).clamp(0.0, 1.0);

                    // Calculate distance
                    let dx = self.cursor_x - prev_x;
                    let dy = self.cursor_y - prev_y;
                    let dist = (dx * dx + dy * dy).sqrt();
                    self.total_distance += dist;

                    self.cursor_trail.push_back((self.cursor_x, self.cursor_y));
                    while self.cursor_trail.len() > 200 {
                        self.cursor_trail.pop_front();
                    }

                    self.add_log_grouped(
                        format!("Move ({:+.1}, {:+.1})", mv.dx, mv.dy),
                        action.confidence,
                        egui::Color32::from_rgb(100, 181, 246),
                        ActionType::Move,
                    );
                }

                // Process clicks
                for btn in &mouse.buttons {
                    self.add_log(
                        format!("{:?} {}", btn.button, if btn.pressed { "pressed" } else { "released" }),
                        action.confidence,
                        egui::Color32::from_rgb(129, 199, 132),
                        ActionType::Click,
                    );
                }

                // Process scroll
                if let Some(ref scroll) = mouse.scroll {
                    self.add_log_grouped(
                        format!("Scroll ({:+.0}, {:+.0})", scroll.dx, scroll.dy),
                        action.confidence,
                        egui::Color32::from_rgb(255, 213, 79),
                        ActionType::Scroll,
                    );
                }
            }

            // Process keyboard actions
            if let Some(ref key) = action.keyboard {
                self.add_log(
                    format!("Key: {:?}", key),
                    action.confidence,
                    egui::Color32::from_rgb(206, 147, 216),
                    ActionType::Key,
                );
            }
        }
    }

    fn add_log(&mut self, description: String, confidence: f32, color: egui::Color32, action_type: ActionType) {
        let now = chrono::Local::now();
        self.log.push_back(ActionLogEntry {
            time_str: now.format("%H:%M:%S%.3f").to_string(),
            timestamp: std::time::Instant::now(),
            description,
            confidence,
            color,
            action_type,
            count: 1,
        });
        while self.log.len() > MAX_LOG_ENTRIES {
            self.log.pop_front();
        }
    }

    fn add_log_grouped(&mut self, description: String, confidence: f32, color: egui::Color32, action_type: ActionType) {
        // Check if we can group with the last entry
        if let Some(last) = self.log.back_mut() {
            if last.action_type == action_type && last.timestamp.elapsed().as_millis() < 500 {
                last.count += 1;
                last.confidence = (last.confidence + confidence) / 2.0; // Average confidence
                return;
            }
        }
        self.add_log(description, confidence, color, action_type);
    }

    fn format_relative_time(elapsed: std::time::Duration) -> String {
        let secs = elapsed.as_secs();
        if secs < 60 {
            format!("{}s ago", secs)
        } else if secs < 3600 {
            format!("{}m ago", secs / 60)
        } else {
            format!("{}h ago", secs / 3600)
        }
    }

    /// Draw empty state when no actions received.
    fn draw_empty_state(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);

            // Placeholder icon (circle with question mark)
            let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 48.0), egui::Sense::hover());
            let painter = ui.painter_at(icon_rect);
            painter.circle_stroke(
                icon_rect.center(),
                20.0,
                egui::Stroke::new(2.0, egui::Color32::from_gray(80)),
            );
            painter.text(
                icon_rect.center(),
                egui::Align2::CENTER_CENTER,
                "?",
                egui::FontId::proportional(24.0),
                egui::Color32::from_gray(80),
            );

            ui.add_space(12.0);
            ui.label(egui::RichText::new("Waiting for decoded actions...").size(14.0).color(egui::Color32::from_gray(150)));
            ui.add_space(4.0);
            ui.label(egui::RichText::new("The decoder will start emitting actions once calibration is complete.").small().weak());
        });
    }

    /// Draw the minimap with enhanced visuals.
    fn draw_minimap(&self, ui: &mut egui::Ui, minimap_size: f32) {
        ui.label(egui::RichText::new("Virtual Cursor").small().strong());

        // Add subtle shadow effect via outer rect
        let shadow_offset = 2.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(minimap_size + shadow_offset, minimap_size + shadow_offset),
            egui::Sense::hover(),
        );

        let main_rect = egui::Rect::from_min_size(rect.min, egui::vec2(minimap_size, minimap_size));

        if !ui.is_rect_visible(rect) {
            return;
        }

        let painter = ui.painter_at(rect);

        // Shadow
        let shadow_rect = main_rect.translate(egui::vec2(shadow_offset, shadow_offset));
        painter.rect_filled(shadow_rect, 6.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 40));

        // Background with rounded corners
        painter.rect_filled(main_rect, 6.0, egui::Color32::from_gray(20));
        painter.rect_stroke(main_rect, 6.0, egui::Stroke::new(1.5, egui::Color32::from_gray(70)));

        // Grid (subtle)
        for i in 1..4 {
            let frac = i as f32 / 4.0;
            let x = main_rect.left() + frac * main_rect.width();
            let y = main_rect.top() + frac * main_rect.height();
            painter.line_segment(
                [egui::pos2(x, main_rect.top()), egui::pos2(x, main_rect.bottom())],
                egui::Stroke::new(0.3, egui::Color32::from_gray(40)),
            );
            painter.line_segment(
                [egui::pos2(main_rect.left(), y), egui::pos2(main_rect.right(), y)],
                egui::Stroke::new(0.3, egui::Color32::from_gray(40)),
            );
        }

        // Center crosshair (subtle dashed style)
        let center_x = main_rect.center().x;
        let center_y = main_rect.center().y;
        let dash_len = 4.0;
        let gap_len = 4.0;

        // Horizontal dashes
        let mut x = main_rect.left() + 4.0;
        while x < main_rect.right() - 4.0 {
            let end_x = (x + dash_len).min(main_rect.right() - 4.0);
            painter.line_segment(
                [egui::pos2(x, center_y), egui::pos2(end_x, center_y)],
                egui::Stroke::new(0.5, egui::Color32::from_gray(50)),
            );
            x += dash_len + gap_len;
        }

        // Vertical dashes
        let mut y = main_rect.top() + 4.0;
        while y < main_rect.bottom() - 4.0 {
            let end_y = (y + dash_len).min(main_rect.bottom() - 4.0);
            painter.line_segment(
                [egui::pos2(center_x, y), egui::pos2(center_x, end_y)],
                egui::Stroke::new(0.5, egui::Color32::from_gray(50)),
            );
            y += dash_len + gap_len;
        }

        // Compass labels
        let label_offset = 6.0;
        let label_color = egui::Color32::from_gray(90);
        let label_font = egui::FontId::proportional(8.0);

        painter.text(
            egui::pos2(main_rect.center().x, main_rect.top() + label_offset),
            egui::Align2::CENTER_TOP,
            "N",
            label_font.clone(),
            label_color,
        );
        painter.text(
            egui::pos2(main_rect.center().x, main_rect.bottom() - label_offset),
            egui::Align2::CENTER_BOTTOM,
            "S",
            label_font.clone(),
            label_color,
        );
        painter.text(
            egui::pos2(main_rect.left() + label_offset, main_rect.center().y),
            egui::Align2::LEFT_CENTER,
            "W",
            label_font.clone(),
            label_color,
        );
        painter.text(
            egui::pos2(main_rect.right() - label_offset, main_rect.center().y),
            egui::Align2::RIGHT_CENTER,
            "E",
            label_font,
            label_color,
        );

        // Heat zone glow around cursor
        if self.cursor_trail.len() > 5 {
            let cx = main_rect.left() + self.cursor_x * main_rect.width();
            let cy = main_rect.top() + self.cursor_y * main_rect.height();

            // Multi-layer glow
            for (radius, alpha) in [(20.0, 15), (14.0, 25), (8.0, 35)] {
                painter.circle_filled(
                    egui::pos2(cx, cy),
                    radius,
                    egui::Color32::from_rgba_unmultiplied(100, 181, 246, alpha),
                );
            }
        }

        // Trail with opacity fade (individual segments)
        if self.show_trail && self.cursor_trail.len() >= 2 {
            let trail_len = self.cursor_trail.len();
            for i in 0..trail_len.saturating_sub(1) {
                let (x0, y0) = self.cursor_trail[i];
                let (x1, y1) = self.cursor_trail[i + 1];

                // Opacity: oldest = nearly transparent, newest = full opacity
                let age_ratio = i as f32 / trail_len as f32;
                let alpha = (age_ratio * 180.0 + 20.0) as u8; // 20 to 200

                let p0 = egui::pos2(
                    main_rect.left() + x0 * main_rect.width(),
                    main_rect.top() + y0 * main_rect.height(),
                );
                let p1 = egui::pos2(
                    main_rect.left() + x1 * main_rect.width(),
                    main_rect.top() + y1 * main_rect.height(),
                );

                painter.line_segment(
                    [p0, p1],
                    egui::Stroke::new(
                        1.0 + age_ratio * 0.5, // Slightly thicker for newer
                        egui::Color32::from_rgba_unmultiplied(100, 181, 246, alpha),
                    ),
                );
            }
        }

        // Cursor crosshair
        let cx = main_rect.left() + self.cursor_x * main_rect.width();
        let cy = main_rect.top() + self.cursor_y * main_rect.height();
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
            egui::pos2(main_rect.left() + 4.0, main_rect.bottom() - 2.0),
            egui::Align2::LEFT_BOTTOM,
            format!("({:.2}, {:.2})", self.cursor_x, self.cursor_y),
            egui::FontId::proportional(9.0),
            egui::Color32::from_gray(120),
        );
    }

    /// Draw movement statistics panel.
    fn draw_stats_panel(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.set_min_width(ui.available_width());
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Movement Stats").small().strong());

                let elapsed = self.session_start.elapsed().as_secs_f32();
                let avg_speed = if elapsed > 0.0 {
                    self.total_distance / elapsed
                } else {
                    0.0
                };

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Distance:").small().weak());
                    ui.label(egui::RichText::new(format!("{:.1} units", self.total_distance * 100.0)).small());
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Avg speed:").small().weak());
                    ui.label(egui::RichText::new(format!("{:.2} units/s", avg_speed * 100.0)).small());
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Position:").small().weak());
                    ui.label(egui::RichText::new(format!("({:.2}, {:.2})", self.cursor_x, self.cursor_y)).small());
                });
            });
        });
    }

    /// Draw action log with enhanced visuals.
    fn draw_action_log(&mut self, ui: &mut egui::Ui, max_height: f32) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Action Log").small().strong());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Time format toggle
                if ui.small_button(if self.show_relative_time { "Abs" } else { "Rel" }).clicked() {
                    self.show_relative_time = !self.show_relative_time;
                }

                // Filter dropdown
                egui::ComboBox::from_id_source("action_filter")
                    .selected_text(self.filter.label())
                    .width(70.0)
                    .show_ui(ui, |ui: &mut egui::Ui| {
                        ui.selectable_value(&mut self.filter, ActionFilter::All, "All");
                        ui.selectable_value(&mut self.filter, ActionFilter::Mouse, "Mouse");
                        ui.selectable_value(&mut self.filter, ActionFilter::Keyboard, "Keyboard");
                        ui.selectable_value(&mut self.filter, ActionFilter::Scroll, "Scroll");
                    });
            });
        });

        egui::ScrollArea::vertical()
            .max_height(max_height)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.log.is_empty() {
                    ui.label(egui::RichText::new("No actions yet...").weak());
                    return;
                }

                let filtered: Vec<_> = self.log.iter()
                    .filter(|e| self.filter.matches(e))
                    .rev()
                    .take(30)
                    .collect();

                if filtered.is_empty() {
                    ui.label(egui::RichText::new("No matching actions...").weak());
                    return;
                }

                for entry in filtered {
                    // Horizontal layout with color-coded left border
                    let (entry_rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 18.0),
                        egui::Sense::hover(),
                    );

                    if ui.is_rect_visible(entry_rect) {
                        let painter = ui.painter_at(entry_rect);

                        // Color-coded left border (2px strip)
                        let border_rect = egui::Rect::from_min_size(
                            entry_rect.min,
                            egui::vec2(2.0, entry_rect.height()),
                        );
                        painter.rect_filled(border_rect, 0.0, entry.color);

                        // Time
                        let time_text = if self.show_relative_time {
                            Self::format_relative_time(entry.timestamp.elapsed())
                        } else {
                            entry.time_str.clone()
                        };

                        let time_width = if self.show_relative_time { 50.0 } else { 85.0 };
                        painter.text(
                            egui::pos2(entry_rect.left() + 6.0, entry_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            &time_text,
                            egui::FontId::monospace(9.0),
                            egui::Color32::from_gray(100),
                        );

                        // Description (with count if grouped)
                        let desc = if entry.count > 1 {
                            format!("{} x{}", entry.description, entry.count)
                        } else {
                            entry.description.clone()
                        };

                        painter.text(
                            egui::pos2(entry_rect.left() + 8.0 + time_width, entry_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            &desc,
                            egui::FontId::proportional(10.0),
                            entry.color,
                        );

                        // Confidence badge
                        let conf_color = if entry.confidence > 0.7 {
                            egui::Color32::from_rgb(76, 175, 80)
                        } else if entry.confidence > 0.4 {
                            egui::Color32::from_rgb(255, 193, 7)
                        } else {
                            egui::Color32::from_rgb(244, 67, 54)
                        };

                        painter.text(
                            egui::pos2(entry_rect.right() - 4.0, entry_rect.center().y),
                            egui::Align2::RIGHT_CENTER,
                            format!("{:.0}%", entry.confidence * 100.0),
                            egui::FontId::proportional(9.0),
                            conf_color,
                        );
                    }
                }
            });
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
                self.total_distance = 0.0;
            }
            if ui.button("Clear Log").clicked() {
                self.log.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("Total: {}", self.total_actions)).small().weak());
            });
        });

        // Check for empty state
        if self.total_actions == 0 && self.log.is_empty() {
            self.draw_empty_state(ui);
            return;
        }

        // Split area: minimap on left, log on right
        let available = ui.available_size();
        let minimap_size = available.y.min(available.x * 0.4).min(180.0);

        ui.horizontal(|ui| {
            // === Cursor minimap ===
            ui.vertical(|ui| {
                self.draw_minimap(ui, minimap_size);

                ui.add_space(4.0);

                // Movement stats panel
                self.draw_stats_panel(ui);
            });

            // === Action log ===
            ui.vertical(|ui| {
                self.draw_action_log(ui, minimap_size + 40.0);
            });
        });
    }
}
