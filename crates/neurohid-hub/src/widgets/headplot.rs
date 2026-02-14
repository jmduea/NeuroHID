//! # Headplot Widget
//!
//! Displays a simple topographic heatmap over the five canonical EEG channels.

use crate::theme;
use crate::widgets::{Widget, WidgetContext, WidgetId};
use eframe::egui;

const CHANNEL_NAMES: [&str; 5] = ["AF3", "AF4", "T7", "T8", "Pz"];
const POS: [(f32, f32); 5] = [
    (0.35, 0.25), // AF3
    (0.65, 0.25), // AF4
    (0.15, 0.50), // T7
    (0.85, 0.50), // T8
    (0.50, 0.70), // Pz
];

pub struct HeadplotWidget {
    window: usize,
    selected_source: Option<String>,
}

impl Default for HeadplotWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl HeadplotWidget {
    pub fn new() -> Self {
        Self {
            window: 128,
            selected_source: None,
        }
    }

    fn band_power(values: &[f32]) -> f32 {
        if values.is_empty() {
            return 0.0;
        }
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32
    }
}

impl Widget for HeadplotWidget {
    fn id(&self) -> WidgetId {
        WidgetId::Headplot
    }

    fn title(&self) -> &str {
        "Headplot"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>, pane_index: usize) {
        let source_options = ctx.candidate_sources_for(WidgetId::Headplot);
        if !source_options.is_empty() {
            let valid = self
                .selected_source
                .as_ref()
                .map(|id| source_options.iter().any(|s| s.id == *id))
                .unwrap_or(false);
            if !valid {
                self.selected_source = Some(source_options[0].id.clone());
            }

            ui.horizontal(|ui| {
                ui.label("Source:");
                let labels: Vec<String> = source_options
                    .iter()
                    .map(|source| format!("{} ({})", source.name, source.id))
                    .collect();
                let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                let mut selected_idx = self
                    .selected_source
                    .as_ref()
                    .and_then(|id| source_options.iter().position(|source| source.id == *id))
                    .unwrap_or(0);
                if theme::select_index(
                    ui,
                    format!("headplot_src_{pane_index}"),
                    &mut selected_idx,
                    &label_refs,
                    190.0,
                ) {
                    self.selected_source = Some(source_options[selected_idx].id.clone());
                }
            });
        }

        let samples =
            ctx.samples_for_widget_source(WidgetId::Headplot, self.selected_source.as_deref());
        if samples.len() < 32 {
            theme::status_chip(ui, "Waiting for EEG samples", theme::Intent::Warning);
            return;
        }

        let start = samples.len().saturating_sub(self.window);
        let mut powers = [0.0f32; 5];
        for (ch, power) in powers.iter_mut().enumerate() {
            let vals: Vec<f32> = samples.range(start..).filter_map(|s| s.get(ch)).collect();
            *power = Self::band_power(&vals);
        }

        let max_p = powers.iter().copied().fold(1e-6f32, f32::max);

        let size = egui::vec2(ui.available_width().min(260.0), 220.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter_at(rect);

        let center = rect.center();
        let radius = rect.width().min(rect.height()) * 0.42;
        painter.circle_filled(center, radius, egui::Color32::from_gray(25));
        painter.circle_stroke(
            center,
            radius,
            egui::Stroke::new(1.0, egui::Color32::from_gray(110)),
        );

        // Nose hint.
        painter.line_segment(
            [
                egui::pos2(center.x - 10.0, center.y - radius + 4.0),
                egui::pos2(center.x, center.y - radius - 10.0),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(140)),
        );
        painter.line_segment(
            [
                egui::pos2(center.x + 10.0, center.y - radius + 4.0),
                egui::pos2(center.x, center.y - radius - 10.0),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(140)),
        );

        for (i, &(nx, ny)) in POS.iter().enumerate() {
            let px = rect.left() + nx * rect.width();
            let py = rect.top() + ny * rect.height();
            let norm = (powers[i] / max_p).clamp(0.0, 1.0);
            let color = egui::Color32::from_rgb(
                (255.0 * norm) as u8,
                (200.0 * (1.0 - (norm - 0.5).abs() * 2.0).max(0.0)) as u8,
                (255.0 * (1.0 - norm)) as u8,
            );

            painter.circle_filled(egui::pos2(px, py), 16.0, color.gamma_multiply(0.55));
            painter.circle_stroke(
                egui::pos2(px, py),
                16.0,
                egui::Stroke::new(1.0, egui::Color32::from_gray(180)),
            );
            painter.text(
                egui::pos2(px, py),
                egui::Align2::CENTER_CENTER,
                CHANNEL_NAMES[i],
                egui::FontId::proportional(10.0),
                egui::Color32::WHITE,
            );
        }

        ui.add_space(4.0);
        theme::status_chip(
            ui,
            "Relative power heatmap (windowed variance)",
            theme::Intent::Muted,
        );
    }
}
