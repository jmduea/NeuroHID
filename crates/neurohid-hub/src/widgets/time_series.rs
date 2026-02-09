//! # Time Series Widget
//!
//! Displays real-time scrolling EEG waveforms for all channels.
//! Each channel is rendered as a separate trace with a unique color.

use crate::widgets::{Widget, WidgetContext, WidgetId};
use eframe::egui;

/// Channel colors matching common EEG GUI conventions.
const CHANNEL_COLORS: &[egui::Color32] = &[
    egui::Color32::from_rgb(129, 199, 132), // green
    egui::Color32::from_rgb(100, 181, 246), // blue
    egui::Color32::from_rgb(239, 154, 154), // red
    egui::Color32::from_rgb(255, 213, 79),  // yellow
    egui::Color32::from_rgb(206, 147, 216), // purple
    egui::Color32::from_rgb(255, 183, 77),  // orange
    egui::Color32::from_rgb(128, 222, 234), // cyan
    egui::Color32::from_rgb(240, 98, 146),  // pink
];

const CHANNEL_NAMES: &[&str] = &["AF3", "AF4", "T7", "T8", "Pz"];

pub struct TimeSeriesWidget {
    /// Vertical scale in µV per division.
    vertical_scale: f32,
    /// Window duration in seconds.
    window_secs: f32,
    /// Which channels are enabled for display.
    channel_enabled: [bool; 8],
}

impl TimeSeriesWidget {
    pub fn new() -> Self {
        Self {
            vertical_scale: 200.0,
            window_secs: 5.0,
            channel_enabled: [true; 8],
        }
    }
}

impl Widget for TimeSeriesWidget {
    fn id(&self) -> WidgetId {
        WidgetId::TimeSeries
    }

    fn title(&self) -> &str {
        "Time Series"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>) {
        // Settings bar
        ui.horizontal(|ui| {
            ui.label("Scale:");
            egui::ComboBox::from_id_source("ts_scale")
                .selected_text(format!("{:.0} µV", self.vertical_scale))
                .width(80.0)
                .show_ui(ui, |ui: &mut egui::Ui| {
                    for &v in &[50.0, 100.0, 200.0, 500.0, 1000.0f32] {
                        ui.selectable_value(&mut self.vertical_scale, v, format!("{:.0} µV", v));
                    }
                });

            ui.label("Window:");
            egui::ComboBox::from_id_source("ts_window")
                .selected_text(format!("{:.0}s", self.window_secs))
                .width(60.0)
                .show_ui(ui, |ui: &mut egui::Ui| {
                    for &v in &[1.0, 2.0, 5.0, 10.0, 20.0, 30.0f32] {
                        ui.selectable_value(&mut self.window_secs, v, format!("{:.0}s", v));
                    }
                });

            // Channel toggles
            ui.separator();
            let num_ch = ctx
                .bus
                .samples
                .back()
                .map(|s| s.channel_count())
                .unwrap_or(5)
                .min(8);
            for ch in 0..num_ch {
                let name = CHANNEL_NAMES.get(ch).unwrap_or(&"?");
                let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];
                let mut enabled = self.channel_enabled[ch];
                if ui
                    .checkbox(
                        &mut enabled,
                        egui::RichText::new(*name).color(color).small(),
                    )
                    .changed()
                {
                    self.channel_enabled[ch] = enabled;
                }
            }
        });

        // Determine how many samples correspond to the window
        let sample_rate = 128.0f32;
        let window_samples = (self.window_secs * sample_rate) as usize;

        // Get the relevant samples from the bus
        let samples = &ctx.bus.samples;
        let start = if samples.len() > window_samples {
            samples.len() - window_samples
        } else {
            0
        };
        let visible_samples: Vec<_> = samples.range(start..).collect();

        if visible_samples.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("No data — waiting for device connection").weak());
            });
            return;
        }

        let num_channels = visible_samples[0].channel_count().min(8);
        let available = ui.available_size();
        let channel_height = available.y / num_channels as f32;

        // Draw each channel
        for ch in 0..num_channels {
            if !self.channel_enabled[ch] {
                // Reserve space but don't draw
                ui.allocate_space(egui::vec2(available.x, channel_height));
                continue;
            }

            let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];
            let (rect, _response) = ui.allocate_exact_size(
                egui::vec2(available.x, channel_height),
                egui::Sense::hover(),
            );

            if ui.is_rect_visible(rect) {
                let painter = ui.painter_at(rect);

                // Background with subtle separator
                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(20));
                painter.line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    egui::Stroke::new(0.5, egui::Color32::from_gray(50)),
                );

                // Channel label
                let name = CHANNEL_NAMES.get(ch).unwrap_or(&"?");
                painter.text(
                    rect.left_top() + egui::vec2(4.0, 2.0),
                    egui::Align2::LEFT_TOP,
                    *name,
                    egui::FontId::proportional(10.0),
                    color,
                );

                // Zero line
                let center_y = rect.center().y;
                painter.line_segment(
                    [
                        egui::pos2(rect.left(), center_y),
                        egui::pos2(rect.right(), center_y),
                    ],
                    egui::Stroke::new(0.5, egui::Color32::from_gray(40)),
                );

                // Plot the waveform
                let n = visible_samples.len();
                if n > 1 {
                    let points: Vec<egui::Pos2> = visible_samples
                        .iter()
                        .enumerate()
                        .filter_map(|(i, sample)| {
                            let value = sample.get(ch)?;
                            let x = rect.left() + (i as f32 / n as f32) * rect.width();
                            // Map value to screen: scale is µV per half-height
                            let y =
                                center_y - (value / self.vertical_scale) * (channel_height * 0.4);
                            let y = y.clamp(rect.top(), rect.bottom());
                            Some(egui::pos2(x, y))
                        })
                        .collect();

                    if points.len() >= 2 {
                        // Downsample for rendering performance if too many points
                        let max_points = (rect.width() as usize * 2).max(1);
                        if points.len() > max_points {
                            let step = points.len() / max_points;
                            let downsampled: Vec<_> =
                                points.iter().step_by(step).copied().collect();
                            painter.add(egui::Shape::line(
                                downsampled,
                                egui::Stroke::new(1.0, color),
                            ));
                        } else {
                            painter.add(egui::Shape::line(points, egui::Stroke::new(1.0, color)));
                        }
                    }
                }
            }
        }
    }
}
