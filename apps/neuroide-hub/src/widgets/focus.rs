//! # Focus Widget
//!
//! Displays a heuristic focus score derived from band energies.

use std::collections::VecDeque;

use crate::theme;
use crate::widgets::{Widget, WidgetContext, WidgetId};
use eframe::egui;

pub struct FocusWidget {
    channel: usize,
    smoothed_focus: f32,
    history: VecDeque<f32>,
    selected_source: Option<String>,
}

impl Default for FocusWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusWidget {
    pub fn new() -> Self {
        Self {
            channel: 0,
            smoothed_focus: 0.0,
            history: VecDeque::new(),
            selected_source: None,
        }
    }

    fn band_energy(values: &[f32], low_hz: f32, high_hz: f32, sample_rate: f32) -> f32 {
        let n = values.len().max(1) as f32;
        let bins = (values.len() / 2).max(2);
        let mut sum = 0.0f32;

        for k in 1..bins {
            let freq = (k as f32 / n) * sample_rate;
            if !(low_hz..high_hz).contains(&freq) {
                continue;
            }
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for (i, &v) in values.iter().enumerate() {
                let phase = 2.0 * std::f32::consts::PI * k as f32 * i as f32 / n;
                re += v * phase.cos();
                im -= v * phase.sin();
            }
            sum += (re * re + im * im) / n.max(1.0);
        }

        sum.max(0.0)
    }
}

impl Widget for FocusWidget {
    fn id(&self) -> WidgetId {
        WidgetId::Focus
    }

    fn title(&self) -> &str {
        "Focus"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>, pane_index: usize) {
        let source_options = ctx.candidate_sources_for(WidgetId::Focus);
        if !source_options.is_empty() {
            let valid = self
                .selected_source
                .as_ref()
                .map(|id| source_options.iter().any(|s| s.id == *id))
                .unwrap_or(false);
            if !valid {
                self.selected_source = Some(source_options[0].id.clone());
            }
        }

        ui.horizontal(|ui| {
            if !source_options.is_empty() {
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
                    format!("focus_src_{pane_index}"),
                    &mut selected_idx,
                    &label_refs,
                    190.0,
                ) {
                    self.selected_source = Some(source_options[selected_idx].id.clone());
                }
            }

            ui.label("Channel:");
            let num_channels = self
                .selected_source
                .as_deref()
                .and_then(|id| ctx.channel_count_for_source(id))
                .or_else(|| ctx.channel_count_for(&["EEG"]))
                .unwrap_or(8)
                .clamp(1, 32);
            if self.channel >= num_channels {
                self.channel = 0;
            }
            let channel_labels: Vec<String> =
                (0..num_channels).map(|ch| format!("{}", ch + 1)).collect();
            let channel_refs: Vec<&str> = channel_labels.iter().map(String::as_str).collect();
            let _ = theme::select_index(
                ui,
                format!("focus_ch_{pane_index}"),
                &mut self.channel,
                &channel_refs,
                80.0,
            );
        });

        let samples =
            ctx.samples_for_widget_source(WidgetId::Focus, self.selected_source.as_deref());
        if samples.len() < 128 {
            theme::status_chip(ui, "Waiting for samples", theme::Intent::Warning);
            return;
        }

        let start = samples.len().saturating_sub(256);
        let values: Vec<f32> = samples
            .range(start..)
            .map(|s| s.get(self.channel).unwrap_or(0.0))
            .collect();

        let sr = 128.0;
        let theta = Self::band_energy(&values, 4.0, 8.0, sr);
        let alpha = Self::band_energy(&values, 8.0, 13.0, sr);
        let beta = Self::band_energy(&values, 13.0, 30.0, sr);

        let raw_focus = beta / (alpha + theta + 1e-6);
        let normalized = (raw_focus / 3.0).clamp(0.0, 1.0);

        self.smoothed_focus = self.smoothed_focus * 0.9 + normalized * 0.1;
        self.history.push_back(self.smoothed_focus);
        while self.history.len() > 240 {
            self.history.pop_front();
        }

        let color = if self.smoothed_focus > 0.66 {
            egui::Color32::from_rgb(76, 175, 80)
        } else if self.smoothed_focus > 0.33 {
            egui::Color32::from_rgb(255, 193, 7)
        } else {
            egui::Color32::from_rgb(244, 67, 54)
        };
        let focus_intent = if self.smoothed_focus > 0.66 {
            theme::Intent::Success
        } else if self.smoothed_focus > 0.33 {
            theme::Intent::Warning
        } else {
            theme::Intent::Danger
        };

        ui.horizontal(|ui| {
            theme::status_chip(
                ui,
                &format!("Focus {:.0}%", self.smoothed_focus * 100.0),
                focus_intent,
            );
            let _ = theme::progress_bar(ui, self.smoothed_focus, ui.available_width());
        });

        ui.horizontal(|ui| {
            ui.label(format!("Theta: {:.2}", theta));
            ui.separator();
            ui.label(format!("Alpha: {:.2}", alpha));
            ui.separator();
            ui.label(format!("Beta: {:.2}", beta));
        });

        let h = 80.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(10.0), h),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, egui::Color32::from_gray(22));

        if self.history.len() >= 2 {
            let points: Vec<egui::Pos2> = self
                .history
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let x =
                        rect.left() + (i as f32 / (self.history.len() - 1) as f32) * rect.width();
                    let y = rect.bottom() - v * rect.height();
                    egui::pos2(x, y)
                })
                .collect();
            painter.add(egui::Shape::line(points, egui::Stroke::new(1.4, color)));
        }
    }
}
