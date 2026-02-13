//! # Focus Widget
//!
//! Displays a heuristic focus score derived from band energies.

use std::collections::VecDeque;

use crate::widgets::{Widget, WidgetContext, WidgetId};
use eframe::egui;

pub struct FocusWidget {
    channel: usize,
    smoothed_focus: f32,
    history: VecDeque<f32>,
    selected_source: Option<String>,
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
                egui::ComboBox::from_id_salt(format!("focus_src_{pane_index}"))
                    .selected_text(
                        self.selected_source
                            .as_deref()
                            .unwrap_or("<auto>")
                            .to_string(),
                    )
                    .show_ui(ui, |ui| {
                        for source in &source_options {
                            ui.selectable_value(
                                &mut self.selected_source,
                                Some(source.id.clone()),
                                format!("{} ({})", source.name, source.id),
                            );
                        }
                    });
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
            egui::ComboBox::from_id_salt(format!("focus_ch_{pane_index}"))
                .selected_text(format!("{}", self.channel + 1))
                .show_ui(ui, |ui| {
                    for ch in 0..num_channels {
                        ui.selectable_value(&mut self.channel, ch, format!("{}", ch + 1));
                    }
                });
        });

        let samples =
            ctx.samples_for_widget_source(WidgetId::Focus, self.selected_source.as_deref());
        if samples.len() < 128 {
            ui.label(egui::RichText::new("Waiting for samples...").weak());
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

        ui.add(
            egui::ProgressBar::new(self.smoothed_focus)
                .text(format!("Focus: {:.0}%", self.smoothed_focus * 100.0))
                .fill(color),
        );

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
