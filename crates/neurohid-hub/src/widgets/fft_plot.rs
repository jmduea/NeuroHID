//! # FFT Plot Widget
//!
//! Displays the power spectral density of the EEG signal in real time.
//! X-axis is frequency (0–64 Hz), Y-axis is amplitude in µV.

use std::collections::VecDeque;
use eframe::egui;
use crate::widgets::{Widget, WidgetContext, WidgetId};

const CHANNEL_COLORS: &[egui::Color32] = &[
    egui::Color32::from_rgb(129, 199, 132),
    egui::Color32::from_rgb(100, 181, 246),
    egui::Color32::from_rgb(239, 154, 154),
    egui::Color32::from_rgb(255, 213, 79),
    egui::Color32::from_rgb(206, 147, 216),
];

const CHANNEL_NAMES: &[&str] = &["AF3", "AF4", "T7", "T8", "Pz"];

/// Number of FFT bins (power of 2).
const FFT_SIZE: usize = 256;

pub struct FftPlotWidget {
    /// Log or linear y-axis.
    log_scale: bool,
    /// Smoothing factor (0 = none, higher = smoother).
    smoothing: f32,
    /// Cached per-channel FFT magnitudes.
    cached_fft: Vec<Vec<f32>>,
    /// Which channels to display.
    channel_enabled: [bool; 5],
    /// Max frequency to display.
    max_freq: f32,
}

impl FftPlotWidget {
    pub fn new() -> Self {
        Self {
            log_scale: true,
            smoothing: 0.7,
            cached_fft: Vec::new(),
            channel_enabled: [true; 5],
            max_freq: 60.0,
        }
    }

    /// Compute FFT magnitude spectrum for a single channel from recent samples.
    fn compute_fft(samples: &VecDeque<neurohid_types::signal::Sample>, channel: usize) -> Vec<f32> {
        let n = FFT_SIZE;
        // Gather the most recent n values for this channel
        let start = if samples.len() > n { samples.len() - n } else { 0 };
        let mut real: Vec<f32> = samples.range(start..)
            .filter_map(|s| s.get(channel))
            .collect();

        if real.len() < n {
            // Not enough data — pad with zeros
            real.resize(n, 0.0);
        } else if real.len() > n {
            real.truncate(n);
        }

        // Apply Hanning window
        for (i, v) in real.iter_mut().enumerate() {
            let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (n as f32 - 1.0)).cos());
            *v *= w;
        }

        // Simple DFT magnitude (not using rustfft to avoid dependency in hub)
        // We only need bins up to Nyquist (n/2)
        let half = n / 2;
        let mut magnitudes = Vec::with_capacity(half);

        for k in 0..half {
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for (j, &x) in real.iter().enumerate() {
                let angle = 2.0 * std::f32::consts::PI * k as f32 * j as f32 / n as f32;
                re += x * angle.cos();
                im -= x * angle.sin();
            }
            let mag = (re * re + im * im).sqrt() / n as f32;
            magnitudes.push(mag);
        }

        magnitudes
    }
}

impl Widget for FftPlotWidget {
    fn id(&self) -> WidgetId {
        WidgetId::FftPlot
    }

    fn title(&self) -> &str {
        "FFT Plot"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>) {
        // Settings bar
        ui.horizontal(|ui| {
            ui.label("Y-Axis:");
            egui::ComboBox::from_id_source("fft_scale")
                .selected_text(if self.log_scale { "Log" } else { "Linear" })
                .width(60.0)
                .show_ui(ui, |ui: &mut egui::Ui| {
                    ui.selectable_value(&mut self.log_scale, true, "Log");
                    ui.selectable_value(&mut self.log_scale, false, "Linear");
                });

            ui.label("Smooth:");
            ui.add(egui::Slider::new(&mut self.smoothing, 0.0..=0.95).max_decimals(2));

            ui.label("Max Hz:");
            egui::ComboBox::from_id_source("fft_maxfreq")
                .selected_text(format!("{:.0}", self.max_freq))
                .width(50.0)
                .show_ui(ui, |ui: &mut egui::Ui| {
                    for &v in &[30.0, 45.0, 60.0, 64.0f32] {
                        ui.selectable_value(&mut self.max_freq, v, format!("{:.0}", v));
                    }
                });

            // Channel toggles
            ui.separator();
            for ch in 0..5 {
                let color = CHANNEL_COLORS[ch];
                let mut enabled = self.channel_enabled[ch];
                if ui.checkbox(&mut enabled, egui::RichText::new(CHANNEL_NAMES[ch]).color(color).small()).changed() {
                    self.channel_enabled[ch] = enabled;
                }
            }
        });

        if ctx.bus.samples.len() < 64 {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Collecting data...").weak());
            });
            return;
        }

        // Compute FFT for each channel
        let num_channels = ctx.bus.samples.back()
            .map(|s| s.channel_count())
            .unwrap_or(5)
            .min(5);

        // Update cached FFT with smoothing
        if self.cached_fft.len() != num_channels {
            self.cached_fft = (0..num_channels)
                .map(|ch| Self::compute_fft(&ctx.bus.samples, ch))
                .collect();
        } else {
            for ch in 0..num_channels {
                let new_fft = Self::compute_fft(&ctx.bus.samples, ch);
                let cached = &mut self.cached_fft[ch];
                if cached.len() != new_fft.len() {
                    *cached = new_fft;
                } else {
                    for (c, &n) in cached.iter_mut().zip(new_fft.iter()) {
                        *c = self.smoothing * *c + (1.0 - self.smoothing) * n;
                    }
                }
            }
        }

        // Draw the FFT plot
        let available = ui.available_size();
        let (rect, _response) = ui.allocate_exact_size(available, egui::Sense::hover());

        if !ui.is_rect_visible(rect) {
            return;
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(20));

        let sample_rate = 128.0;
        let nyquist = sample_rate / 2.0;
        let freq_range = self.max_freq.min(nyquist);
        let half_bins = FFT_SIZE / 2;
        let bins_to_show = ((freq_range / nyquist) * half_bins as f32) as usize;

        // Find global max for y-axis scaling
        let mut y_max = 1.0e-6f32;
        for ch in 0..num_channels {
            if !self.channel_enabled[ch] { continue; }
            for &v in self.cached_fft[ch].iter().take(bins_to_show) {
                y_max = y_max.max(v);
            }
        }

        // Draw frequency grid lines
        let freq_ticks = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0f32];
        for &freq in &freq_ticks {
            if freq > freq_range { break; }
            let x = rect.left() + (freq / freq_range) * rect.width();
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(0.5, egui::Color32::from_gray(40)),
            );
            painter.text(
                egui::pos2(x, rect.bottom() - 2.0),
                egui::Align2::CENTER_BOTTOM,
                format!("{:.0}", freq),
                egui::FontId::proportional(9.0),
                egui::Color32::from_gray(120),
            );
        }

        // Draw band shading
        let bands: &[(&str, f32, f32, egui::Color32)] = &[
            ("δ", 0.5, 4.0, egui::Color32::from_rgba_premultiplied(100, 100, 200, 15)),
            ("θ", 4.0, 8.0, egui::Color32::from_rgba_premultiplied(100, 200, 100, 15)),
            ("α", 8.0, 13.0, egui::Color32::from_rgba_premultiplied(200, 200, 100, 15)),
            ("β", 13.0, 30.0, egui::Color32::from_rgba_premultiplied(200, 100, 100, 15)),
            ("γ", 30.0, 45.0, egui::Color32::from_rgba_premultiplied(200, 100, 200, 15)),
        ];
        for &(label, f_lo, f_hi, fill) in bands {
            if f_lo > freq_range { continue; }
            let x0 = rect.left() + (f_lo / freq_range) * rect.width();
            let x1 = rect.left() + (f_hi.min(freq_range) / freq_range) * rect.width();
            let band_rect = egui::Rect::from_min_max(
                egui::pos2(x0, rect.top()),
                egui::pos2(x1, rect.bottom()),
            );
            painter.rect_filled(band_rect, 0.0, fill);
            painter.text(
                egui::pos2((x0 + x1) / 2.0, rect.top() + 2.0),
                egui::Align2::CENTER_TOP,
                label,
                egui::FontId::proportional(9.0),
                egui::Color32::from_gray(80),
            );
        }

        // Draw channel FFT lines
        for ch in 0..num_channels {
            if !self.channel_enabled[ch] { continue; }
            let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];
            let fft = &self.cached_fft[ch];

            let points: Vec<egui::Pos2> = (0..bins_to_show)
                .map(|i| {
                    let freq = i as f32 * (nyquist / half_bins as f32);
                    let x = rect.left() + (freq / freq_range) * rect.width();
                    let val = fft.get(i).copied().unwrap_or(0.0).max(1e-10);
                    let y_norm = if self.log_scale {
                        let log_val = val.log10();
                        let log_max = y_max.log10();
                        let log_min = log_max - 4.0; // 4 decades of range
                        ((log_val - log_min) / (log_max - log_min)).clamp(0.0, 1.0)
                    } else {
                        (val / y_max).clamp(0.0, 1.0)
                    };
                    let y = rect.bottom() - y_norm * rect.height();
                    egui::pos2(x, y)
                })
                .collect();

            if points.len() >= 2 {
                painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
            }
        }

        // Axis labels
        painter.text(
            egui::pos2(rect.center().x, rect.bottom()),
            egui::Align2::CENTER_BOTTOM,
            "Frequency (Hz)",
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(100),
        );
    }
}
