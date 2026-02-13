//! # Accelerometer Widget
//!
//! Displays motion stream values (X/Y/Z + magnitude) and short trend bars.

use crate::widgets::{Widget, WidgetContext, WidgetId};
use eframe::egui;

pub struct AccelerometerWidget {
    window_samples: usize,
    selected_source: Option<String>,
}

impl AccelerometerWidget {
    pub fn new() -> Self {
        Self {
            window_samples: 128,
            selected_source: None,
        }
    }

    fn axis_stats(values: &[f32]) -> (f32, f32, f32) {
        if values.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        let latest = *values.last().unwrap_or(&0.0);
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let rms =
            (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt();
        (latest, mean, rms)
    }
}

impl Widget for AccelerometerWidget {
    fn id(&self) -> WidgetId {
        WidgetId::Accelerometer
    }

    fn title(&self) -> &str {
        "Accelerometer"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>, pane_index: usize) {
        let motion_streams: Vec<_> = ctx
            .snapshot
            .discovered_streams
            .iter()
            .filter(|s| {
                let t = s.stream_type.to_ascii_lowercase();
                t.contains("motion") || t.contains("acc") || t.contains("imu")
            })
            .collect();

        if !motion_streams.is_empty() {
            if self.selected_source.is_none() {
                self.selected_source = Some(motion_streams[0].id.clone());
            }
            ui.horizontal(|ui| {
                ui.label("Source:");
                egui::ComboBox::from_id_salt(format!("acc_src_{}", pane_index))
                    .selected_text(
                        self.selected_source
                            .clone()
                            .unwrap_or_else(|| "<auto>".to_string()),
                    )
                    .show_ui(ui, |ui| {
                        for stream in &motion_streams {
                            ui.selectable_value(
                                &mut self.selected_source,
                                Some(stream.id.clone()),
                                format!("{} ({})", stream.name, stream.id),
                            );
                        }
                    });
            });
        }

        let samples = self
            .selected_source
            .as_deref()
            .and_then(|id| ctx.samples_for_source(id))
            .unwrap_or_else(|| ctx.samples_for(WidgetId::Accelerometer));
        if samples.is_empty() {
            ui.label(egui::RichText::new("No motion stream samples yet").weak());
            return;
        }

        let start = samples.len().saturating_sub(self.window_samples);
        let window: Vec<_> = samples.range(start..).collect();

        let mut x_vals = Vec::with_capacity(window.len());
        let mut y_vals = Vec::with_capacity(window.len());
        let mut z_vals = Vec::with_capacity(window.len());
        let mut mag_vals = Vec::with_capacity(window.len());

        for sample in &window {
            let x = sample.get(0).unwrap_or(0.0);
            let y = sample.get(1).unwrap_or(0.0);
            let z = sample.get(2).unwrap_or(0.0);
            x_vals.push(x);
            y_vals.push(y);
            z_vals.push(z);
            mag_vals.push((x * x + y * y + z * z).sqrt());
        }

        let (x, x_mean, x_rms) = Self::axis_stats(&x_vals);
        let (y, y_mean, y_rms) = Self::axis_stats(&y_vals);
        let (z, z_mean, z_rms) = Self::axis_stats(&z_vals);
        let (m, _m_mean, m_rms) = Self::axis_stats(&mag_vals);

        ui.horizontal(|ui| {
            ui.group(|ui| {
                ui.label(egui::RichText::new("Latest").small().strong());
                ui.label(format!("X: {x:.3}"));
                ui.label(format!("Y: {y:.3}"));
                ui.label(format!("Z: {z:.3}"));
                ui.label(format!("|v|: {m:.3}"));
            });

            ui.group(|ui| {
                ui.label(egui::RichText::new("RMS (window)").small().strong());
                ui.label(format!("X rms: {x_rms:.3}"));
                ui.label(format!("Y rms: {y_rms:.3}"));
                ui.label(format!("Z rms: {z_rms:.3}"));
                ui.label(format!("|v| rms: {m_rms:.3}"));
            });
        });

        ui.add_space(8.0);
        ui.label(egui::RichText::new("Axis trends").small().strong());

        let desired_h = 120.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(10.0), desired_h),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, egui::Color32::from_gray(22));

        let max_abs = x_vals
            .iter()
            .chain(y_vals.iter())
            .chain(z_vals.iter())
            .fold(1e-3f32, |acc, v| acc.max(v.abs()));

        let draw = |vals: &[f32], color: egui::Color32, painter: &egui::Painter| {
            if vals.len() < 2 {
                return;
            }
            let points: Vec<egui::Pos2> = vals
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    let x = rect.left() + (i as f32 / (vals.len() - 1) as f32) * rect.width();
                    let y = rect.center().y - (*v / max_abs) * rect.height() * 0.45;
                    egui::pos2(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(points, egui::Stroke::new(1.3, color)));
        };

        draw(&x_vals, egui::Color32::from_rgb(100, 181, 246), &painter);
        draw(&y_vals, egui::Color32::from_rgb(129, 199, 132), &painter);
        draw(&z_vals, egui::Color32::from_rgb(239, 154, 154), &painter);

        painter.line_segment(
            [
                egui::pos2(rect.left(), rect.center().y),
                egui::pos2(rect.right(), rect.center().y),
            ],
            egui::Stroke::new(0.5, egui::Color32::from_gray(70)),
        );

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "Means: X {x_mean:.3}  Y {y_mean:.3}  Z {z_mean:.3}  | {} samples",
                window.len()
            ))
            .small()
            .weak(),
        );
    }
}
