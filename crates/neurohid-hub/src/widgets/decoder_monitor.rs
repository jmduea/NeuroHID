//! # Decoder Monitor Widget
//!
//! Displays decoder performance metrics: action probability distribution,
//! confidence gauge, decoded-actions-per-minute, and feature-vector summary.

use crate::theme;
use crate::widgets::channel_meta::EEG_CHANNEL_NAMES as CHANNEL_NAMES;
use crate::widgets::{Widget, WidgetContext, WidgetId};
use eframe::egui;
use neurohid_types::action::Action;
use std::collections::VecDeque;
use std::f32::consts::PI;

/// History depth for actions-per-minute calculation.
const ACTION_HISTORY_SECS: f64 = 60.0;

/// Feature vector summary groups (band-power blocks per channel).
const CHANNELS: usize = 5;
const BANDS: usize = 5;
const BAND_NAMES: &[&str] = &["d", "th", "a", "b", "g"];

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
    /// Session start time for uptime tracking.
    session_start: std::time::Instant,
    /// Last action timestamp for latency estimation.
    last_action_time: Option<std::time::Instant>,
    /// Estimated processing latency in ms.
    processing_latency_ms: f32,
}

impl Default for DecoderMonitorWidget {
    fn default() -> Self {
        Self::new()
    }
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
            session_start: std::time::Instant::now(),
            last_action_time: None,
            processing_latency_ms: 0.0,
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

        let instant_now = std::time::Instant::now();

        // Process actions from the bus
        for action in ctx.bus.actions.iter() {
            let class = Self::classify_action(action);
            self.action_type_counts[class] += 1;

            if !action.is_none() {
                self.action_count += 1;
                self.action_timestamps.push_back(now);

                // Update latency estimation
                if let Some(last) = self.last_action_time {
                    let delta = instant_now.duration_since(last).as_secs_f32() * 1000.0;
                    self.processing_latency_ms = self.processing_latency_ms * 0.8 + delta * 0.2;
                }
                self.last_action_time = Some(instant_now);
            }

            // Update confidence tracking
            self.avg_confidence =
                self.smoothing * self.avg_confidence + (1.0 - self.smoothing) * action.confidence;
            self.confidence_history.push_back(action.confidence);
        }

        // Trim old timestamps
        let cutoff = now - ACTION_HISTORY_SECS;
        while self.action_timestamps.front().is_some_and(|&t| t < cutoff) {
            self.action_timestamps.pop_front();
        }

        // Keep confidence history bounded
        while self.confidence_history.len() > 200 {
            self.confidence_history.pop_front();
        }
    }

    /// Draw an arc gauge for confidence visualization.
    fn draw_arc_gauge(&self, ui: &mut egui::Ui, pane_index: usize) {
        let size = egui::vec2(70.0, 70.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());

        if !ui.is_rect_visible(rect) {
            return;
        }

        let painter = ui.painter_at(rect);
        let center = rect.center();
        let radius = 28.0;
        let thickness = 6.0;

        // Arc parameters: -135deg to +135deg (270deg sweep)
        let start_angle = -135.0_f32.to_radians() - PI / 2.0;
        let end_angle = 135.0_f32.to_radians() - PI / 2.0;
        let sweep = end_angle - start_angle;

        // Background arc
        let segments = 60;
        for i in 0..segments {
            let t0 = i as f32 / segments as f32;
            let t1 = (i + 1) as f32 / segments as f32;
            let a0 = start_angle + t0 * sweep;
            let a1 = start_angle + t1 * sweep;

            let p0 = center + egui::vec2(a0.cos() * radius, a0.sin() * radius);
            let p1 = center + egui::vec2(a1.cos() * radius, a1.sin() * radius);

            painter.line_segment(
                [p0, p1],
                egui::Stroke::new(thickness, egui::Color32::from_gray(40)),
            );
        }

        // Filled arc based on confidence
        let conf = self.avg_confidence.clamp(0.0, 1.0);
        let fill_segments = (conf * segments as f32) as usize;

        for i in 0..fill_segments {
            let t0 = i as f32 / segments as f32;
            let t1 = (i + 1) as f32 / segments as f32;
            let a0 = start_angle + t0 * sweep;
            let a1 = start_angle + t1 * sweep;

            // Color gradient: red (0-40%), yellow (40-70%), green (70-100%)
            let progress = t1;
            let color = if progress < 0.4 {
                egui::Color32::from_rgb(244, 67, 54) // Red
            } else if progress < 0.7 {
                // Interpolate red to yellow
                let t = (progress - 0.4) / 0.3;
                egui::Color32::from_rgb(
                    (244.0 + (255.0 - 244.0) * t) as u8,
                    (67.0 + (193.0 - 67.0) * t) as u8,
                    (54.0 - 47.0 * t) as u8,
                )
            } else {
                // Interpolate yellow to green
                let t = (progress - 0.7) / 0.3;
                egui::Color32::from_rgb(
                    (255.0 - (255.0 - 76.0) * t) as u8,
                    (193.0 - (193.0 - 175.0) * t) as u8,
                    (7.0 + (80.0 - 7.0) * t) as u8,
                )
            };

            let p0 = center + egui::vec2(a0.cos() * radius, a0.sin() * radius);
            let p1 = center + egui::vec2(a1.cos() * radius, a1.sin() * radius);

            painter.line_segment([p0, p1], egui::Stroke::new(thickness, color));
        }

        // Center text
        let conf_color = if conf > 0.7 {
            egui::Color32::from_rgb(76, 175, 80)
        } else if conf > 0.4 {
            egui::Color32::from_rgb(255, 193, 7)
        } else {
            egui::Color32::from_rgb(244, 67, 54)
        };

        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            format!("{:.0}%", conf * 100.0),
            egui::FontId::proportional(16.0),
            conf_color,
        );

        // Tooltip on hover
        if response.hovered() {
            let _ = egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                egui::Id::new(format!("conf_gauge_tooltip_{}", pane_index)),
                egui::PopupAnchor::Pointer,
            )
            .gap(12.0)
            .show(|ui| {
                ui.label(format!("Confidence: {:.1}%", conf * 100.0));
            });
        }
    }

    /// Draw confidence history chart with threshold lines and gradient fill.
    fn draw_confidence_chart(&self, ui: &mut egui::Ui) {
        let available_width = ui.available_width();
        let chart_height = 80.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(available_width, chart_height),
            egui::Sense::hover(),
        );

        if !ui.is_rect_visible(rect) {
            return;
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, egui::Color32::from_gray(25));

        // Threshold lines at 40% and 70%
        let y_40 = rect.bottom() - 0.4 * rect.height();
        let y_70 = rect.bottom() - 0.7 * rect.height();

        painter.line_segment(
            [
                egui::pos2(rect.left(), y_40),
                egui::pos2(rect.right() - 40.0, y_40),
            ],
            egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(244, 67, 54, 60)),
        );
        painter.line_segment(
            [
                egui::pos2(rect.left(), y_70),
                egui::pos2(rect.right() - 40.0, y_70),
            ],
            egui::Stroke::new(0.5, egui::Color32::from_rgba_unmultiplied(76, 175, 80, 60)),
        );

        // Threshold labels
        painter.text(
            egui::pos2(rect.right() - 38.0, y_40),
            egui::Align2::LEFT_CENTER,
            "40%",
            egui::FontId::proportional(8.0),
            egui::Color32::from_rgba_unmultiplied(244, 67, 54, 100),
        );
        painter.text(
            egui::pos2(rect.right() - 38.0, y_70),
            egui::Align2::LEFT_CENTER,
            "70%",
            egui::FontId::proportional(8.0),
            egui::Color32::from_rgba_unmultiplied(76, 175, 80, 100),
        );

        if self.confidence_history.len() >= 2 {
            let chart_width = rect.width() - 45.0; // Leave room for labels
            let points: Vec<egui::Pos2> = self
                .confidence_history
                .iter()
                .enumerate()
                .map(|(i, &c)| {
                    let x = rect.left()
                        + (i as f32 / self.confidence_history.len() as f32) * chart_width;
                    let y = rect.bottom() - c.clamp(0.0, 1.0) * rect.height();
                    egui::pos2(x, y)
                })
                .collect();

            // Gradient fill under the line
            for i in 0..points.len().saturating_sub(1) {
                let p0 = points[i];
                let p1 = points[i + 1];
                let bottom0 = egui::pos2(p0.x, rect.bottom());
                let bottom1 = egui::pos2(p1.x, rect.bottom());

                // Draw vertical gradient strips
                let steps = 10;
                for s in 0..steps {
                    let t0 = s as f32 / steps as f32;
                    let t1 = (s + 1) as f32 / steps as f32;
                    let alpha = ((1.0 - t0) * 40.0) as u8;

                    let y0 = p0.y + (bottom0.y - p0.y) * t0;
                    let y1 = p0.y + (bottom0.y - p0.y) * t1;
                    let y2 = p1.y + (bottom1.y - p1.y) * t1;
                    let y3 = p1.y + (bottom1.y - p1.y) * t0;

                    let mesh = egui::Mesh {
                        indices: vec![0, 1, 2, 0, 2, 3],
                        vertices: vec![
                            egui::epaint::Vertex {
                                pos: egui::pos2(p0.x, y0),
                                uv: egui::epaint::WHITE_UV,
                                color: egui::Color32::from_rgba_unmultiplied(100, 181, 246, alpha),
                            },
                            egui::epaint::Vertex {
                                pos: egui::pos2(p0.x, y1),
                                uv: egui::epaint::WHITE_UV,
                                color: egui::Color32::from_rgba_unmultiplied(
                                    100,
                                    181,
                                    246,
                                    alpha.saturating_sub(4),
                                ),
                            },
                            egui::epaint::Vertex {
                                pos: egui::pos2(p1.x, y2),
                                uv: egui::epaint::WHITE_UV,
                                color: egui::Color32::from_rgba_unmultiplied(
                                    100,
                                    181,
                                    246,
                                    alpha.saturating_sub(4),
                                ),
                            },
                            egui::epaint::Vertex {
                                pos: egui::pos2(p1.x, y3),
                                uv: egui::epaint::WHITE_UV,
                                color: egui::Color32::from_rgba_unmultiplied(100, 181, 246, alpha),
                            },
                        ],
                        texture_id: egui::TextureId::default(),
                    };
                    painter.add(egui::Shape::mesh(mesh));
                }
            }

            // Main line
            painter.add(egui::Shape::line(
                points.clone(),
                egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 181, 246)),
            ));

            // Min/max/avg labels
            let min_conf = self
                .confidence_history
                .iter()
                .cloned()
                .fold(f32::INFINITY, f32::min);
            let max_conf = self
                .confidence_history
                .iter()
                .cloned()
                .fold(f32::NEG_INFINITY, f32::max);
            let avg_conf: f32 =
                self.confidence_history.iter().sum::<f32>() / self.confidence_history.len() as f32;

            let label_x = rect.right() - 2.0;
            painter.text(
                egui::pos2(label_x, rect.top() + 8.0),
                egui::Align2::RIGHT_TOP,
                format!("max:{:.0}", max_conf * 100.0),
                egui::FontId::proportional(8.0),
                egui::Color32::from_gray(120),
            );
            painter.text(
                egui::pos2(label_x, rect.center().y),
                egui::Align2::RIGHT_CENTER,
                format!("avg:{:.0}", avg_conf * 100.0),
                egui::FontId::proportional(8.0),
                egui::Color32::from_gray(120),
            );
            painter.text(
                egui::pos2(label_x, rect.bottom() - 8.0),
                egui::Align2::RIGHT_BOTTOM,
                format!("min:{:.0}", min_conf * 100.0),
                egui::FontId::proportional(8.0),
                egui::Color32::from_gray(120),
            );
        }
    }

    /// Draw action type distribution with improved visuals.
    fn draw_action_distribution(&self, ui: &mut egui::Ui, pane_index: usize) {
        let labels = ["Move", "Click", "Scroll", "Key", "Noop"];
        let colors = [
            egui::Color32::from_rgb(100, 181, 246),
            egui::Color32::from_rgb(129, 199, 132),
            egui::Color32::from_rgb(255, 213, 79),
            egui::Color32::from_rgb(206, 147, 216),
            egui::Color32::from_gray(100),
        ];
        let total: u64 = self.action_type_counts.iter().sum();

        // Stacked bar (16px height)
        let bar_width = 140.0;
        let (bar_rect, bar_response) =
            ui.allocate_exact_size(egui::vec2(bar_width, 16.0), egui::Sense::hover());
        let painter = ui.painter_at(bar_rect);
        painter.rect_filled(bar_rect, 4.0, egui::Color32::from_gray(30));

        if total > 0 {
            let mut x = bar_rect.left();
            let mut hovered_segment: Option<(usize, f32)> = None;

            for (i, &count) in self.action_type_counts.iter().enumerate() {
                let frac = count as f32 / total as f32;
                let w = frac * bar_rect.width();
                if w > 0.5 {
                    let seg = egui::Rect::from_min_max(
                        egui::pos2(x, bar_rect.top()),
                        egui::pos2(x + w, bar_rect.bottom()),
                    );
                    painter.rect_filled(seg, if i == 0 { 4.0 } else { 0.0 }, colors[i]);

                    // Percentage label inside segment if wide enough (>25px)
                    if w > 25.0 {
                        let pct = frac * 100.0;
                        painter.text(
                            seg.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("{:.0}%", pct),
                            egui::FontId::proportional(9.0),
                            egui::Color32::from_gray(20),
                        );
                    }

                    // Check hover
                    if let Some(hover_pos) = bar_response.hover_pos()
                        && seg.contains(hover_pos)
                    {
                        hovered_segment = Some((i, frac));
                    }
                }
                x += w;
            }

            // Tooltip for hovered segment
            if let Some((idx, frac)) = hovered_segment {
                let _ = egui::Tooltip::always_open(
                    ui.ctx().clone(),
                    ui.layer_id(),
                    egui::Id::new(format!("action_dist_tooltip_{}", pane_index)),
                    egui::PopupAnchor::Pointer,
                )
                .gap(12.0)
                .show(|ui| {
                    ui.label(format!(
                        "{}: {} ({:.1}%)",
                        labels[idx],
                        self.action_type_counts[idx],
                        frac * 100.0
                    ));
                });
            }
        }

        // Legend with counts
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for (i, &label) in labels.iter().enumerate() {
                let count = self.action_type_counts[i];
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let (dot_rect, _) =
                        ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(dot_rect.center(), 3.0, colors[i]);
                    ui.label(egui::RichText::new(format!("{} ({})", label, count)).small());
                });
            }
        });
    }

    /// Draw feature heatmap with tooltips and color scale legend.
    fn draw_feature_heatmap(&self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>, pane_index: usize) {
        if let Some(fv) = ctx.bus.features.back() {
            ui.label(egui::RichText::new("Feature Heatmap").small().strong());
            ui.label(
                egui::RichText::new(format!("{}D vector", fv.dim()))
                    .small()
                    .weak(),
            );

            let available_width = ui.available_width();
            let cell_size = ((available_width - 30.0) / (BANDS as f32 + 1.5)).min(36.0);
            let heatmap_height = cell_size * CHANNELS as f32;
            let legend_width = 20.0;
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(
                    cell_size * (BANDS + 1) as f32 + legend_width + 10.0,
                    heatmap_height + 20.0,
                ),
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
                for (b, band_name) in BAND_NAMES.iter().enumerate().take(BANDS) {
                    let x = rect.left() + cell_size + b as f32 * cell_size;
                    painter.text(
                        egui::pos2(x + cell_size / 2.0, rect.top()),
                        egui::Align2::CENTER_TOP,
                        *band_name,
                        egui::FontId::proportional(10.0),
                        egui::Color32::from_gray(150),
                    );
                }

                // Track hovered cell for tooltip
                let mut hovered_cell: Option<(usize, usize, f32)> = None;

                // Perceptually uniform colormap function
                let get_color = |norm: f32| -> egui::Color32 {
                    // Dark blue -> Teal/Cyan -> Yellow/Warm
                    let r = (norm * 255.0).min(255.0) as u8;
                    let g = ((norm * 2.0).min(1.0) * 200.0) as u8;
                    let b = ((1.0 - norm) * 255.0).min(255.0) as u8;
                    egui::Color32::from_rgb(r, g, b)
                };

                // Draw cells
                for (ch, channel_name) in CHANNEL_NAMES.iter().enumerate().take(CHANNELS) {
                    let y = rect.top() + 16.0 + ch as f32 * cell_size;

                    // Channel label
                    painter.text(
                        egui::pos2(rect.left() + cell_size / 2.0, y + cell_size / 2.0),
                        egui::Align2::CENTER_CENTER,
                        *channel_name,
                        egui::FontId::proportional(9.0),
                        egui::Color32::from_gray(150),
                    );

                    for b in 0..BANDS {
                        let idx = ch * BANDS + b;
                        if idx < vals.len() {
                            let val = vals[idx];
                            let norm = ((val - v_min) / v_range).clamp(0.0, 1.0);
                            let color = get_color(norm);

                            let cell_rect = egui::Rect::from_min_size(
                                egui::pos2(
                                    rect.left() + cell_size + b as f32 * cell_size + 1.0,
                                    y + 1.0,
                                ),
                                egui::vec2(cell_size - 2.0, cell_size - 2.0),
                            );

                            // Cell background with border
                            painter.rect_filled(cell_rect, 3.0, color);
                            painter.rect_stroke(
                                cell_rect,
                                3.0,
                                egui::Stroke::new(1.0, egui::Color32::from_gray(50)),
                                egui::StrokeKind::Outside,
                            );

                            // Check hover
                            if let Some(hover_pos) = response.hover_pos()
                                && cell_rect.contains(hover_pos)
                            {
                                hovered_cell = Some((ch, b, val));
                                // Highlight hovered cell
                                painter.rect_stroke(
                                    cell_rect,
                                    3.0,
                                    egui::Stroke::new(2.0, egui::Color32::WHITE),
                                    egui::StrokeKind::Outside,
                                );
                            }
                        }
                    }
                }

                // Color scale legend bar on the right
                let legend_x = rect.left() + cell_size * (BANDS + 1) as f32 + 8.0;
                let legend_top = rect.top() + 16.0;
                let legend_height = heatmap_height;

                for i in 0..20 {
                    let t = 1.0 - (i as f32 / 19.0);
                    let color = get_color(t);
                    let seg_rect = egui::Rect::from_min_size(
                        egui::pos2(legend_x, legend_top + i as f32 * (legend_height / 20.0)),
                        egui::vec2(legend_width - 4.0, legend_height / 20.0 + 1.0),
                    );
                    painter.rect_filled(seg_rect, 0.0, color);
                }

                // Legend labels
                painter.text(
                    egui::pos2(legend_x + legend_width - 2.0, legend_top),
                    egui::Align2::LEFT_TOP,
                    format!("{:.1}", v_max),
                    egui::FontId::proportional(8.0),
                    egui::Color32::from_gray(120),
                );
                painter.text(
                    egui::pos2(legend_x + legend_width - 2.0, legend_top + legend_height),
                    egui::Align2::LEFT_BOTTOM,
                    format!("{:.1}", v_min),
                    egui::FontId::proportional(8.0),
                    egui::Color32::from_gray(120),
                );

                // Tooltip for hovered cell
                if let Some((ch, band, val)) = hovered_cell {
                    let _ = egui::Tooltip::always_open(
                        ui.ctx().clone(),
                        ui.layer_id(),
                        egui::Id::new(format!("heatmap_tooltip_{}", pane_index)),
                        egui::PopupAnchor::Pointer,
                    )
                    .gap(12.0)
                    .show(|ui| {
                        ui.label(format!(
                            "{} {} : {:.4}",
                            CHANNEL_NAMES[ch], BAND_NAMES[band], val
                        ));
                    });
                }
            }
        } else {
            theme::status_chip(ui, "No feature data yet", theme::Intent::Warning);
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

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>, pane_index: usize) {
        self.process_new_actions(ctx);

        let apm = self.action_timestamps.len() as f32; // actions in last 60s

        // Top stats row
        ui.horizontal(|ui| {
            // Confidence arc gauge
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Confidence").small().strong());
                    self.draw_arc_gauge(ui, pane_index);
                });
            });

            // Actions per minute
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Actions/min").small().strong());
                    ui.label(egui::RichText::new(format!("{:.0}", apm)).size(20.0));
                    ui.label(
                        egui::RichText::new(format!("Total: {}", self.action_count))
                            .small()
                            .weak(),
                    );
                });
            });

            // Action type distribution
            ui.group(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Action Types").small().strong());
                    self.draw_action_distribution(ui, pane_index);
                });
            });
        });

        // Stats section
        ui.separator();
        ui.horizontal(|ui| {
            // Session uptime
            let elapsed = self.session_start.elapsed();
            let mins = elapsed.as_secs() / 60;
            let secs = elapsed.as_secs() % 60;

            ui.label(
                egui::RichText::new(format!("Session: {}m {:02}s", mins, secs))
                    .small()
                    .weak(),
            );

            ui.separator();

            ui.label(
                egui::RichText::new(format!("Total actions: {}", self.action_count))
                    .small()
                    .weak(),
            );

            ui.separator();

            ui.label(
                egui::RichText::new(format!("Latency: {:.0}ms", self.processing_latency_ms))
                    .small()
                    .weak(),
            );
        });

        ui.separator();

        // Confidence history chart
        ui.label(egui::RichText::new("Confidence History").small().strong());
        self.draw_confidence_chart(ui);

        ui.separator();

        // Feature vector heatmap
        self.draw_feature_heatmap(ui, ctx, pane_index);
    }
}
