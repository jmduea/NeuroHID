//! # Decoder Monitor Widget
//!
//! Displays decoder performance metrics: action probability distribution,
//! confidence gauge, decoded-actions-per-minute, and feature-vector summary.

use std::collections::VecDeque;
use eframe::egui;
use neurohid_types::action::Action;
use crate::widgets::{Widget, WidgetContext, WidgetId};

/// History depth for actions-per-minute calculation.
const ACTION_HISTORY_SECS: f64 = 60.0;

/// Feature vector summary groups (band-power blocks per channel).
const CHANNELS: usize = 5;
const BANDS: usize = 5;
const BAND_NAMES: &[&str] = &["δ", "θ", "α", "β", "γ"];
const CHANNEL_NAMES: &[&str] = &["AF3", "AF4", "T7", "T8", "Pz"];

pub struct DecoderMonitorWidget {
    /// Recent action timestamps for computing actions-per-minute.
    action_timestamps: VecDeque<f64>,
    /// Rolling confidence average.
    avg_confidence: f32,
    /// Smoothing factor.
    smoothing: f32,
    /// Number of non-noop actions seen.
    action_count: u64,
    /// Recent confidence values for mini-chart.
    confidence_history: VecDeque<f32>,
    /// Counts of each action type: [mouse_move, click, scroll, key, noop].
    action_type_counts: [u64; 5],
}

impl DecoderMonitorWidget {
    pub fn new() -> Self {
        Self {
            action_timestamps: VecDeque::new(),
            avg_confidence: 0.0,
            smoothing: 0.9,
            action_count: 0,
            confidence_history: VecDeque::new(),
            action_type_counts: [0; 5],
        }
    }

    fn classify_action(action: &Action) -> usize {
        if action.is_none() {
            4 // noop
        } else if action.keyboard.is_some() {
            3 // key
        } else if let Some(ref mouse) = action.mouse {
            if mouse.scroll.is_some() {
                2 // scroll
            } else if !mouse.buttons.is_empty() {
                1 // click
            } else {
                0 // move
            }
        } else {
            4 // noop
        }
    }

    fn process_new_actions(&mut self, ctx: &WidgetContext<'_>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        // Process actions from the bus
        for action in ctx.bus.actions.iter() {
            let class = Self::classify_action(action);
            self.action_type_counts[class] += 1;

            if !action.is_none() {
                self.action_count += 1;
                self.action_timestamps.push_back(now);
            }

            // Update confidence tracking
            self.avg_confidence =
                self.smoothing * self.avg_confidence + (1.0 - self.smoothing) * action.confidence;
            self.confidence_history.push_back(action.confidence);
        }

        // Trim old timestamps
        let cutoff = now - ACTION_HISTORY_SECS;
        while self.action_timestamps.front().map_or(false, |&t| t < cutoff) {
            self.action_timestamps.pop_front();
        }

        // Keep confidence history bounded
        while self.confidence_history.len() > 200 {
            self.confidence_history.pop_front();
        }
    }
}

impl Widget for DecoderMonitorWidget {
    fn id(&self) -> WidgetId {
        WidgetId::DecoderMonitor
    }

    fn title(&self) -> &str {
        "Decoder Monitor"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>) {
        self.process_new_actions(ctx);

        let apm = self.action_timestamps.len() as f32; // actions in last 60s

        // Top stats row
        ui.horizontal(|ui| {
            // Confidence gauge
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Confidence").small().strong());
                    let conf = self.avg_confidence;
                    let color = if conf > 0.7 {
                        egui::Color32::from_rgb(76, 175, 80)
                    } else if conf > 0.4 {
                        egui::Color32::from_rgb(255, 193, 7)
                    } else {
                        egui::Color32::from_rgb(244, 67, 54)
                    };
                    ui.label(egui::RichText::new(format!("{:.1}%", conf * 100.0)).color(color).size(20.0));
                    let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(80.0, 6.0), egui::Sense::hover());
                    let painter = ui.painter_at(bar_rect);
                    painter.rect_filled(bar_rect, 3.0, egui::Color32::from_gray(40));
                    let fill = egui::Rect::from_min_max(
                        bar_rect.min,
                        egui::pos2(bar_rect.left() + bar_rect.width() * conf, bar_rect.max.y),
                    );
                    painter.rect_filled(fill, 3.0, color);
                });
            });

            // Actions per minute
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Actions/min").small().strong());
                    ui.label(egui::RichText::new(format!("{:.0}", apm)).size(20.0));
                    ui.label(egui::RichText::new(format!("Total: {}", self.action_count)).small().weak());
                });
            });

            // Action type distribution
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Action Types").small().strong());
                    let labels = ["Move", "Click", "Scroll", "Key", "Noop"];
                    let colors = [
                        egui::Color32::from_rgb(100, 181, 246),
                        egui::Color32::from_rgb(129, 199, 132),
                        egui::Color32::from_rgb(255, 213, 79),
                        egui::Color32::from_rgb(206, 147, 216),
                        egui::Color32::from_gray(100),
                    ];
                    let total: u64 = self.action_type_counts.iter().sum();

                    // Mini stacked bar
                    let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(120.0, 12.0), egui::Sense::hover());
                    let painter = ui.painter_at(bar_rect);
                    painter.rect_filled(bar_rect, 3.0, egui::Color32::from_gray(30));

                    if total > 0 {
                        let mut x = bar_rect.left();
                        for (i, &count) in self.action_type_counts.iter().enumerate() {
                            let frac = count as f32 / total as f32;
                            let w = frac * bar_rect.width();
                            if w > 0.5 {
                                let seg = egui::Rect::from_min_max(
                                    egui::pos2(x, bar_rect.top()),
                                    egui::pos2(x + w, bar_rect.bottom()),
                                );
                                painter.rect_filled(seg, 0.0, colors[i]);
                            }
                            x += w;
                        }
                    }

                    // Legend
                    ui.horizontal(|ui| {
                        for (i, &label) in labels.iter().enumerate() {
                            ui.colored_label(colors[i], egui::RichText::new(label).small());
                        }
                    });
                });
            });
        });

        ui.separator();

        // Confidence history chart
        ui.label(egui::RichText::new("Confidence History").small().strong());
        {
            let available_width = ui.available_width();
            let chart_height = 60.0;
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(available_width, chart_height),
                egui::Sense::hover(),
            );

            if ui.is_rect_visible(rect) {
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 4.0, egui::Color32::from_gray(25));

                // 0.5 threshold line
                let y_half = rect.bottom() - 0.5 * rect.height();
                painter.line_segment(
                    [egui::pos2(rect.left(), y_half), egui::pos2(rect.right(), y_half)],
                    egui::Stroke::new(0.5, egui::Color32::from_gray(50)),
                );

                if self.confidence_history.len() >= 2 {
                    let points: Vec<egui::Pos2> = self.confidence_history.iter()
                        .enumerate()
                        .map(|(i, &c)| {
                            let x = rect.left() + (i as f32 / self.confidence_history.len() as f32) * rect.width();
                            let y = rect.bottom() - c.clamp(0.0, 1.0) * rect.height();
                            egui::pos2(x, y)
                        })
                        .collect();

                    painter.add(egui::Shape::line(
                        points,
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 181, 246)),
                    ));
                }
            }
        }

        ui.separator();

        // Feature vector heatmap (if available)
        if let Some(fv) = ctx.bus.features.back() {
            ui.label(egui::RichText::new("Feature Heatmap").small().strong());
            ui.label(egui::RichText::new(format!("{}D vector", fv.dim())).small().weak());

            let available_width = ui.available_width();
            let cell_size = (available_width / (BANDS as f32 + 1.0)).min(30.0);
            let heatmap_height = cell_size * CHANNELS as f32;
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(cell_size * (BANDS + 1) as f32, heatmap_height + 16.0),
                egui::Sense::hover(),
            );

            if ui.is_rect_visible(rect) {
                let painter = ui.painter_at(rect);

                // Find min/max for color scaling
                let vals = &fv.values;
                let v_min = vals.iter().cloned().fold(f32::INFINITY, f32::min);
                let v_max = vals.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                let v_range = (v_max - v_min).max(1e-6);

                // Draw header row
                for b in 0..BANDS {
                    let x = rect.left() + cell_size + b as f32 * cell_size;
                    painter.text(
                        egui::pos2(x + cell_size / 2.0, rect.top()),
                        egui::Align2::CENTER_TOP,
                        BAND_NAMES[b],
                        egui::FontId::proportional(9.0),
                        egui::Color32::from_gray(150),
                    );
                }

                // Draw cells
                for ch in 0..CHANNELS {
                    let y = rect.top() + 14.0 + ch as f32 * cell_size;

                    // Channel label
                    painter.text(
                        egui::pos2(rect.left() + cell_size / 2.0, y + cell_size / 2.0),
                        egui::Align2::CENTER_CENTER,
                        CHANNEL_NAMES[ch],
                        egui::FontId::proportional(8.0),
                        egui::Color32::from_gray(150),
                    );

                    for b in 0..BANDS {
                        let idx = ch * BANDS + b; // Simplified indexing into feature vector
                        if idx < vals.len() {
                            let norm = ((vals[idx] - v_min) / v_range).clamp(0.0, 1.0);
                            let r = (norm * 255.0) as u8;
                            let g = ((1.0 - (norm - 0.5).abs() * 2.0).max(0.0) * 200.0) as u8;
                            let b_color = ((1.0 - norm) * 255.0) as u8;
                            let color = egui::Color32::from_rgb(r, g, b_color);

                            let cell_rect = egui::Rect::from_min_size(
                                egui::pos2(rect.left() + cell_size + b as f32 * cell_size, y),
                                egui::vec2(cell_size - 1.0, cell_size - 1.0),
                            );
                            painter.rect_filled(cell_rect, 2.0, color);
                        }
                    }
                }
            }
        } else {
            ui.label(egui::RichText::new("No feature data yet").weak());
        }
    }
}
