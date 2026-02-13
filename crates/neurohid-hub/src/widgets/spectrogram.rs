//! # Spectrogram Widget
//!
//! Simple real-time spectrogram using a small DFT over the recent EEG window.

use crate::widgets::{Widget, WidgetContext, WidgetId};
use eframe::egui;

const MAX_ROWS: usize = 120;
const MAX_BINS: usize = 32;

pub struct SpectrogramWidget {
    channel: usize,
    window_samples: usize,
    hop_samples: usize,
    history: Vec<Vec<f32>>, // newest at end
    last_consumed_samples: u64,
    selected_source: Option<String>,
}

impl Default for SpectrogramWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectrogramWidget {
    pub fn new() -> Self {
        Self {
            channel: 0,
            window_samples: 128,
            hop_samples: 16,
            history: Vec::new(),
            last_consumed_samples: 0,
            selected_source: None,
        }
    }

    fn dft_bins(values: &[f32], bins: usize) -> Vec<f32> {
        let n = values.len().max(1) as f32;
        let mut out = Vec::with_capacity(bins);
        for k in 0..bins {
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            let freq = k as f32;
            for (i, &v) in values.iter().enumerate() {
                let phase = 2.0 * std::f32::consts::PI * freq * i as f32 / n;
                re += v * phase.cos();
                im -= v * phase.sin();
            }
            out.push((re * re + im * im).sqrt() / n);
        }
        out
    }

    fn color_for_norm(norm: f32) -> egui::Color32 {
        let t = norm.clamp(0.0, 1.0);
        let r = (255.0 * t) as u8;
        let g = (220.0 * (t * t).sqrt()) as u8;
        let b = (255.0 * (1.0 - t)) as u8;
        egui::Color32::from_rgb(r, g, b)
    }
}

impl Widget for SpectrogramWidget {
    fn id(&self) -> WidgetId {
        WidgetId::Spectrogram
    }

    fn title(&self) -> &str {
        "Spectrogram"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>, pane_index: usize) {
        let source_options = ctx.candidate_sources_for(WidgetId::Spectrogram);
        let mut source_changed = false;
        if !source_options.is_empty() {
            let valid = self
                .selected_source
                .as_ref()
                .map(|id| source_options.iter().any(|s| s.id == *id))
                .unwrap_or(false);
            if !valid {
                self.selected_source = Some(source_options[0].id.clone());
                source_changed = true;
            }
        }

        ui.horizontal(|ui| {
            if !source_options.is_empty() {
                ui.label("Src:");
                let before = self.selected_source.clone();
                egui::ComboBox::from_id_salt(format!("spec_src_{pane_index}"))
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
                if self.selected_source != before {
                    source_changed = true;
                }
            }

            ui.label("Ch:");
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
            egui::ComboBox::from_id_salt(format!("spec_ch_{pane_index}"))
                .selected_text(format!("{}", self.channel + 1))
                .show_ui(ui, |ui| {
                    for ch in 0..num_channels {
                        ui.selectable_value(&mut self.channel, ch, format!("{}", ch + 1));
                    }
                });

            ui.label("Window:");
            ui.add(
                egui::DragValue::new(&mut self.window_samples)
                    .range(32..=512)
                    .speed(1),
            );

            ui.label("Hop:");
            ui.add(
                egui::DragValue::new(&mut self.hop_samples)
                    .range(4..=128)
                    .speed(1),
            );
        });

        if source_changed {
            self.history.clear();
            self.last_consumed_samples = 0;
        }

        let samples =
            ctx.samples_for_widget_source(WidgetId::Spectrogram, self.selected_source.as_deref());
        if samples.len() < self.window_samples {
            ui.label(egui::RichText::new("Waiting for enough samples...").weak());
            return;
        }

        let total = ctx.bus.total_samples_received;
        let delta = total.saturating_sub(self.last_consumed_samples);
        if delta >= self.hop_samples as u64 {
            let start = samples.len().saturating_sub(self.window_samples);
            let window: Vec<f32> = samples
                .range(start..)
                .map(|s| s.get(self.channel).unwrap_or(0.0))
                .collect();
            let bins = (self.window_samples / 2).clamp(8, MAX_BINS);
            let row = Self::dft_bins(&window, bins);
            self.history.push(row);
            while self.history.len() > MAX_ROWS {
                self.history.remove(0);
            }
            self.last_consumed_samples = total;
        }

        if self.history.is_empty() {
            ui.label(egui::RichText::new("No spectrogram rows yet").weak());
            return;
        }

        let bins = self.history[0].len();
        let max_v = self
            .history
            .iter()
            .flat_map(|row| row.iter().copied())
            .fold(1e-6f32, f32::max);

        let height = 180.0;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(10.0), height),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, egui::Color32::from_gray(18));

        let rows = self.history.len();
        for (r, row) in self.history.iter().enumerate() {
            for (c, val) in row.iter().enumerate() {
                let x0 = rect.left() + (r as f32 / rows as f32) * rect.width();
                let x1 = rect.left() + ((r + 1) as f32 / rows as f32) * rect.width();
                let y0 = rect.bottom() - ((c + 1) as f32 / bins as f32) * rect.height();
                let y1 = rect.bottom() - (c as f32 / bins as f32) * rect.height();
                let cell = egui::Rect::from_min_max(egui::pos2(x0, y0), egui::pos2(x1, y1));
                let norm = *val / max_v;
                painter.rect_filled(cell, 0.0, Self::color_for_norm(norm));
            }
        }

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "rows={} bins={} max={max_v:.4} (channel {})",
                self.history.len(),
                bins,
                self.channel + 1
            ))
            .small()
            .weak(),
        );
    }
}
