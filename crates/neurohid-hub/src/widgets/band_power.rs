//! # Band Power Widget
//!
//! Displays power in standard EEG frequency bands as bar charts.
//! Bands: Delta (0.5–4), Theta (4–8), Alpha (8–13), Beta (13–30), Gamma (30–45).

use std::collections::VecDeque;
use eframe::egui;
use crate::widgets::{Widget, WidgetContext, WidgetId};

const CHANNEL_NAMES: &[&str] = &["AF3", "AF4", "T7", "T8", "Pz"];

/// Standard EEG frequency bands.
const BANDS: &[(&str, f32, f32, egui::Color32)] = &[
    ("Delta", 0.5, 4.0, egui::Color32::from_rgb(100, 100, 220)),
    ("Theta", 4.0, 8.0, egui::Color32::from_rgb(100, 200, 100)),
    ("Alpha", 8.0, 13.0, egui::Color32::from_rgb(220, 220, 80)),
    ("Beta", 13.0, 30.0, egui::Color32::from_rgb(220, 130, 80)),
    ("Gamma", 30.0, 45.0, egui::Color32::from_rgb(180, 100, 220)),
];

/// FFT size for band-power computation.
const FFT_SIZE: usize = 256;

pub struct BandPowerWidget {
    /// Show relative (%) or absolute (µV²) power.
    relative: bool,
    /// Which channel is selected (or "All").
    selected_channel: Option<usize>,
    /// Smoothing factor.
    smoothing: f32,
    /// Cached band powers: [channel][band].
    cached_powers: Vec<[f32; 5]>,
}

impl BandPowerWidget {
    pub fn new() -> Self {
        Self {
            relative: true,
            selected_channel: None,
            smoothing: 0.8,
            cached_powers: Vec::new(),
        }
    }

    /// Compute band powers for one channel from raw FFT magnitude bins.
    fn compute_band_powers(
        samples: &VecDeque<neurohid_types::signal::Sample>,
        channel: usize,
    ) -> [f32; 5] {
        let n = FFT_SIZE;
        let start = if samples.len() > n { samples.len() - n } else { 0 };
        let mut real: Vec<f32> = samples.range(start..)
            .filter_map(|s| s.get(channel))
            .collect();

        if real.len() < n {
            real.resize(n, 0.0);
        } else if real.len() > n {
            real.truncate(n);
        }

        // Apply Hanning window
        for (i, v) in real.iter_mut().enumerate() {
            let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (n as f32 - 1.0)).cos());
            *v *= w;
        }

        let sample_rate = 128.0f32;
        let nyquist = sample_rate / 2.0;
        let half = n / 2;
        let freq_per_bin = nyquist / half as f32;

        // Compute power spectrum (only need bins in our frequency range)
        let max_bin = ((45.0 / freq_per_bin) as usize).min(half);
        let mut power_spectrum = vec![0.0f32; max_bin];

        for k in 0..max_bin {
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for (j, &x) in real.iter().enumerate() {
                let angle = 2.0 * std::f32::consts::PI * k as f32 * j as f32 / n as f32;
                re += x * angle.cos();
                im -= x * angle.sin();
            }
            power_spectrum[k] = (re * re + im * im) / (n as f32 * n as f32);
        }

        // Sum power in each band
        let mut band_powers = [0.0f32; 5];
        for (b, &(_name, f_lo, f_hi, _color)) in BANDS.iter().enumerate() {
            let bin_lo = (f_lo / freq_per_bin) as usize;
            let bin_hi = ((f_hi / freq_per_bin) as usize).min(max_bin);
            for k in bin_lo..bin_hi {
                if k < power_spectrum.len() {
                    band_powers[b] += power_spectrum[k];
                }
            }
        }

        band_powers
    }
}

impl Widget for BandPowerWidget {
    fn id(&self) -> WidgetId {
        WidgetId::BandPower
    }

    fn title(&self) -> &str {
        "Band Power"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>) {
        // Settings bar
        ui.horizontal(|ui| {
            ui.label("Display:");
            egui::ComboBox::from_id_source("bp_mode")
                .selected_text(if self.relative { "Relative %" } else { "Absolute" })
                .width(80.0)
                .show_ui(ui, |ui: &mut egui::Ui| {
                    ui.selectable_value(&mut self.relative, true, "Relative %");
                    ui.selectable_value(&mut self.relative, false, "Absolute");
                });

            ui.label("Channel:");
            let ch_text = self.selected_channel
                .map(|ch| CHANNEL_NAMES.get(ch).unwrap_or(&"?").to_string())
                .unwrap_or_else(|| "All".to_string());
            egui::ComboBox::from_id_source("bp_channel")
                .selected_text(&ch_text)
                .width(60.0)
                .show_ui(ui, |ui: &mut egui::Ui| {
                    ui.selectable_value(&mut self.selected_channel, None, "All");
                    for (i, name) in CHANNEL_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut self.selected_channel, Some(i), *name);
                    }
                });

            ui.label("Smooth:");
            ui.add(egui::Slider::new(&mut self.smoothing, 0.0..=0.95).max_decimals(2));
        });

        if ctx.bus.samples.len() < 64 {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Collecting data...").weak());
            });
            return;
        }

        // Determine which channels to show
        let num_channels = ctx.bus.samples.back()
            .map(|s| s.channel_count())
            .unwrap_or(5)
            .min(5);

        let channels: Vec<usize> = match self.selected_channel {
            Some(ch) if ch < num_channels => vec![ch],
            _ => (0..num_channels).collect(),
        };

        // Update cached powers with smoothing
        if self.cached_powers.len() != num_channels {
            self.cached_powers = (0..num_channels)
                .map(|ch| Self::compute_band_powers(&ctx.bus.samples, ch))
                .collect();
        } else {
            for ch in 0..num_channels {
                let new = Self::compute_band_powers(&ctx.bus.samples, ch);
                let cached = &mut self.cached_powers[ch];
                for b in 0..5 {
                    cached[b] = self.smoothing * cached[b] + (1.0 - self.smoothing) * new[b];
                }
            }
        }

        // Draw bar chart
        let available = ui.available_size();
        let (rect, _response) = ui.allocate_exact_size(available, egui::Sense::hover());

        if !ui.is_rect_visible(rect) {
            return;
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(20));

        let num_groups = channels.len();
        let num_bands = 5;
        let group_spacing = 8.0;
        let bar_spacing = 1.0;
        let total_groups_width = rect.width() - group_spacing * (num_groups + 1) as f32;
        let group_width = total_groups_width / num_groups as f32;
        let bar_width = (group_width - bar_spacing * (num_bands - 1) as f32) / num_bands as f32;

        for (g, &ch) in channels.iter().enumerate() {
            let group_x = rect.left() + group_spacing + (group_width + group_spacing) * g as f32;

            // Channel label
            painter.text(
                egui::pos2(group_x + group_width / 2.0, rect.bottom() - 2.0),
                egui::Align2::CENTER_BOTTOM,
                CHANNEL_NAMES.get(ch).unwrap_or(&"?"),
                egui::FontId::proportional(10.0),
                egui::Color32::from_gray(180),
            );

            let powers = &self.cached_powers[ch];
            let total_power: f32 = powers.iter().sum();
            let chart_bottom = rect.bottom() - 16.0;
            let chart_top = rect.top() + 16.0;
            let chart_height = chart_bottom - chart_top;

            // Find max for scaling
            let y_max = if self.relative {
                100.0
            } else {
                powers.iter().cloned().fold(f32::EPSILON, f32::max)
            };

            for b in 0..num_bands {
                let val = if self.relative && total_power > 0.0 {
                    (powers[b] / total_power) * 100.0
                } else {
                    powers[b]
                };

                let bar_height = (val / y_max * chart_height).min(chart_height).max(0.0);
                let bar_x = group_x + (bar_width + bar_spacing) * b as f32;
                let bar_rect = egui::Rect::from_min_max(
                    egui::pos2(bar_x, chart_bottom - bar_height),
                    egui::pos2(bar_x + bar_width, chart_bottom),
                );

                let (_name, _, _, color) = BANDS[b];
                painter.rect_filled(bar_rect, 2.0, color);

                // Band label on top of bar (if enough space)
                if bar_width > 12.0 {
                    painter.text(
                        egui::pos2(bar_x + bar_width / 2.0, chart_bottom - bar_height - 2.0),
                        egui::Align2::CENTER_BOTTOM,
                        if self.relative { format!("{:.0}%", val) } else { format!("{:.1}", val) },
                        egui::FontId::proportional(8.0),
                        egui::Color32::from_gray(180),
                    );
                }
            }
        }

        // Band legend at top
        let legend_y = rect.top() + 4.0;
        let mut legend_x = rect.left() + 8.0;
        for &(name, _, _, color) in BANDS {
            let dot_rect = egui::Rect::from_min_size(
                egui::pos2(legend_x, legend_y),
                egui::vec2(8.0, 8.0),
            );
            painter.rect_filled(dot_rect, 2.0, color);
            legend_x += 10.0;
            painter.text(
                egui::pos2(legend_x, legend_y),
                egui::Align2::LEFT_TOP,
                name,
                egui::FontId::proportional(9.0),
                egui::Color32::from_gray(150),
            );
            legend_x += 40.0;
        }
    }
}
