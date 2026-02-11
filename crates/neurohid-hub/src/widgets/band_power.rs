//! # Band Power Widget
//!
//! Displays power in standard EEG frequency bands as bar charts.
//! Bands: Delta (0.5-4), Theta (4-8), Alpha (8-13), Beta (13-30), Gamma (30-45).

use std::collections::VecDeque;
use eframe::egui;
use crate::widgets::{Widget, WidgetContext, WidgetId};

const CHANNEL_NAMES: &[&str] = &["AF3", "AF4", "T7", "T8", "Pz"];

/// Standard EEG frequency bands with name, freq range, and color.
const BANDS: &[(&str, f32, f32, egui::Color32)] = &[
    ("Delta", 0.5, 4.0, egui::Color32::from_rgb(100, 100, 220)),
    ("Theta", 4.0, 8.0, egui::Color32::from_rgb(100, 200, 100)),
    ("Alpha", 8.0, 13.0, egui::Color32::from_rgb(220, 220, 80)),
    ("Beta", 13.0, 30.0, egui::Color32::from_rgb(220, 130, 80)),
    ("Gamma", 30.0, 45.0, egui::Color32::from_rgb(180, 100, 220)),
];

/// FFT size for band-power computation.
const FFT_SIZE: usize = 256;

/// Number of history samples for sparklines (~60 seconds at 2Hz update).
const HISTORY_SIZE: usize = 120;

pub struct BandPowerWidget {
    /// Show relative (%) or absolute (uV^2) power.
    relative: bool,
    /// Which channel is selected (or "All").
    selected_channel: Option<usize>,
    /// Smoothing factor.
    smoothing: f32,
    /// Cached band powers: [channel][band].
    cached_powers: Vec<[f32; 5]>,
    /// Band visibility toggle for legend interaction.
    band_visible: [bool; 5],
    /// Power history for sparklines: [band] -> recent power values (averaged across visible channels).
    power_history: Vec<VecDeque<f32>>,
    /// Frame counter for throttling history updates.
    frame_count: u32,
}

impl BandPowerWidget {
    pub fn new() -> Self {
        Self {
            relative: true,
            selected_channel: None,
            smoothing: 0.8,
            cached_powers: Vec::new(),
            band_visible: [true; 5],
            power_history: (0..5).map(|_| VecDeque::with_capacity(HISTORY_SIZE)).collect(),
            frame_count: 0,
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

    /// Find the dominant band index and its percentage.
    fn find_dominant_band(&self, channels: &[usize]) -> (usize, f32) {
        let mut band_totals = [0.0f32; 5];
        let mut grand_total = 0.0f32;

        for &ch in channels {
            if ch < self.cached_powers.len() {
                for b in 0..5 {
                    if self.band_visible[b] {
                        band_totals[b] += self.cached_powers[ch][b];
                        grand_total += self.cached_powers[ch][b];
                    }
                }
            }
        }

        let mut dominant_idx = 0;
        let mut max_power = 0.0f32;
        for (b, &power) in band_totals.iter().enumerate() {
            if power > max_power && self.band_visible[b] {
                max_power = power;
                dominant_idx = b;
            }
        }

        let pct = if grand_total > 0.0 {
            (max_power / grand_total) * 100.0
        } else {
            0.0
        };

        (dominant_idx, pct)
    }

    /// Brighten a color for hover highlight.
    fn brighten_color(color: egui::Color32, factor: f32) -> egui::Color32 {
        egui::Color32::from_rgb(
            ((color.r() as f32 * factor).min(255.0)) as u8,
            ((color.g() as f32 * factor).min(255.0)) as u8,
            ((color.b() as f32 * factor).min(255.0)) as u8,
        )
    }

    /// Darken a color for gradient bottom.
    fn darken_color(color: egui::Color32, factor: f32) -> egui::Color32 {
        egui::Color32::from_rgb(
            ((color.r() as f32 * factor)) as u8,
            ((color.g() as f32 * factor)) as u8,
            ((color.b() as f32 * factor)) as u8,
        )
    }

    /// Draw a sparkline for band power history.
    fn draw_sparkline(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        band_idx: usize,
        color: egui::Color32,
    ) {
        let history = &self.power_history[band_idx];
        if history.len() < 2 {
            return;
        }

        // Find min/max for scaling
        let min_val = history.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_val = history.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let range = (max_val - min_val).max(0.001);

        let points: Vec<egui::Pos2> = history.iter().enumerate().map(|(i, &val)| {
            let x = rect.left() + (i as f32 / (HISTORY_SIZE - 1) as f32) * rect.width();
            let y = rect.bottom() - ((val - min_val) / range) * rect.height();
            egui::pos2(x, y)
        }).collect();

        // Draw as polyline
        let stroke = egui::Stroke::new(1.5, color);
        for window in points.windows(2) {
            painter.line_segment([window[0], window[1]], stroke);
        }
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
        self.frame_count = self.frame_count.wrapping_add(1);

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

        if ctx.samples_for(WidgetId::BandPower).len() < 64 {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Collecting data...").weak());
            });
            return;
        }

        // Determine which channels to show
        let eeg_samples = ctx.samples_for(WidgetId::BandPower);
        let num_channels = eeg_samples.back()
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
                .map(|ch| Self::compute_band_powers(eeg_samples, ch))
                .collect();
        } else {
            for ch in 0..num_channels {
                let new = Self::compute_band_powers(eeg_samples, ch);
                let cached = &mut self.cached_powers[ch];
                for b in 0..5 {
                    cached[b] = self.smoothing * cached[b] + (1.0 - self.smoothing) * new[b];
                }
            }
        }

        // Update power history (throttled to ~2Hz)
        if self.frame_count % 30 == 0 {
            for b in 0..5 {
                let avg_power: f32 = channels.iter()
                    .filter_map(|&ch| self.cached_powers.get(ch).map(|p| p[b]))
                    .sum::<f32>() / channels.len().max(1) as f32;

                let history = &mut self.power_history[b];
                if history.len() >= HISTORY_SIZE {
                    history.pop_front();
                }
                history.push_back(avg_power);
            }
        }

        // Find dominant band
        let (dominant_band, dominant_pct) = self.find_dominant_band(&channels);

        // Calculate layout dimensions
        let available = ui.available_size();
        let sparkline_height = 40.0;
        let legend_height = 20.0;
        let dominant_height = 18.0;
        let chart_height = available.y - sparkline_height - legend_height - dominant_height - 16.0;

        // --- Dominant Band Indicator ---
        let (dominant_rect, _) = ui.allocate_exact_size(
            egui::vec2(available.x, dominant_height),
            egui::Sense::hover(),
        );
        let dominant_painter = ui.painter_at(dominant_rect);
        let (band_name, _, _, band_color) = BANDS[dominant_band];

        // Subtle glow background
        let glow_rect = dominant_rect.shrink2(egui::vec2(dominant_rect.width() * 0.3, 2.0));
        dominant_painter.rect_filled(
            glow_rect,
            4.0,
            egui::Color32::from_rgba_unmultiplied(band_color.r(), band_color.g(), band_color.b(), 30),
        );

        dominant_painter.text(
            dominant_rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("Dominant: {} ({:.0}%)", band_name, dominant_pct),
            egui::FontId::proportional(12.0),
            band_color,
        );

        // --- Interactive Legend ---
        let (legend_rect, _) = ui.allocate_exact_size(
            egui::vec2(available.x, legend_height),
            egui::Sense::hover(),
        );
        let legend_painter = ui.painter_at(legend_rect);
        let legend_y = legend_rect.center().y;
        let mut legend_x = legend_rect.left() + 8.0;

        for (b, &(name, f_lo, f_hi, color)) in BANDS.iter().enumerate() {
            let box_size = 10.0;
            let text_width = 45.0;
            let item_width = box_size + 4.0 + text_width;

            let item_rect = egui::Rect::from_min_size(
                egui::pos2(legend_x, legend_rect.top()),
                egui::vec2(item_width, legend_height),
            );

            // Check for click to toggle visibility
            let item_response = ui.allocate_rect(item_rect, egui::Sense::click());
            if item_response.clicked() {
                self.band_visible[b] = !self.band_visible[b];
            }

            // Draw colored square
            let box_rect = egui::Rect::from_min_size(
                egui::pos2(legend_x, legend_y - box_size / 2.0),
                egui::vec2(box_size, box_size),
            );

            let display_color = if self.band_visible[b] {
                color
            } else {
                egui::Color32::from_gray(80)
            };

            legend_painter.rect_filled(box_rect, 2.0, display_color);

            // Draw text
            let text_color = if self.band_visible[b] {
                egui::Color32::from_gray(180)
            } else {
                egui::Color32::from_gray(100)
            };

            legend_painter.text(
                egui::pos2(legend_x + box_size + 4.0, legend_y),
                egui::Align2::LEFT_CENTER,
                name,
                egui::FontId::proportional(9.0),
                text_color,
            );

            // Tooltip on hover
            if item_response.hovered() {
                egui::show_tooltip_at_pointer(ui.ctx(), egui::Id::new(format!("legend_tip_{}", b)), |ui| {
                    ui.label(format!("{}: {:.1}-{:.1} Hz", name, f_lo, f_hi));
                    ui.label(if self.band_visible[b] { "Click to hide" } else { "Click to show" });
                });
            }

            legend_x += item_width + 8.0;
        }

        // --- Bar Chart Area ---
        let (chart_rect, _) = ui.allocate_exact_size(
            egui::vec2(available.x, chart_height.max(60.0)),
            egui::Sense::hover(),
        );

        if !ui.is_rect_visible(chart_rect) {
            return;
        }

        let chart_painter = ui.painter_at(chart_rect);
        chart_painter.rect_filled(chart_rect, 0.0, egui::Color32::from_gray(20));

        let num_groups = channels.len();
        let num_bands = 5;
        let group_spacing = 8.0;
        let bar_spacing = 1.0;
        let total_groups_width = chart_rect.width() - group_spacing * (num_groups + 1) as f32;
        let group_width = total_groups_width / num_groups as f32;
        let bar_width = (group_width - bar_spacing * (num_bands - 1) as f32) / num_bands as f32;

        // Track hovered bar for tooltip
        let pointer_pos = ui.ctx().pointer_hover_pos();

        for (g, &ch) in channels.iter().enumerate() {
            let group_x = chart_rect.left() + group_spacing + (group_width + group_spacing) * g as f32;

            // Channel label
            chart_painter.text(
                egui::pos2(group_x + group_width / 2.0, chart_rect.bottom() - 2.0),
                egui::Align2::CENTER_BOTTOM,
                CHANNEL_NAMES.get(ch).unwrap_or(&"?"),
                egui::FontId::proportional(10.0),
                egui::Color32::from_gray(180),
            );

            let powers = &self.cached_powers[ch];
            let total_power: f32 = powers.iter().enumerate()
                .filter(|(b, _)| self.band_visible[*b])
                .map(|(_, p)| p)
                .sum();
            let chart_bottom = chart_rect.bottom() - 16.0;
            let chart_top = chart_rect.top() + 8.0;
            let bar_chart_height = chart_bottom - chart_top;

            // Find max for scaling (only visible bands)
            let y_max = if self.relative {
                100.0
            } else {
                powers.iter().enumerate()
                    .filter(|(b, _)| self.band_visible[*b])
                    .map(|(_, p)| *p)
                    .fold(f32::EPSILON, f32::max)
            };

            for b in 0..num_bands {
                if !self.band_visible[b] {
                    continue;
                }

                let val = if self.relative && total_power > 0.0 {
                    (powers[b] / total_power) * 100.0
                } else {
                    powers[b]
                };

                let bar_height = (val / y_max * bar_chart_height).min(bar_chart_height).max(0.0);
                let bar_x = group_x + (bar_width + bar_spacing) * b as f32;
                let bar_rect = egui::Rect::from_min_max(
                    egui::pos2(bar_x, chart_bottom - bar_height),
                    egui::pos2(bar_x + bar_width, chart_bottom),
                );

                let (name, f_lo, f_hi, base_color) = BANDS[b];

                // Check if this bar is hovered
                let is_hovered = pointer_pos.map(|p| bar_rect.contains(p)).unwrap_or(false);
                let is_dominant = b == dominant_band;

                // Determine bar color with hover/dominant highlighting
                let bar_color = if is_hovered {
                    Self::brighten_color(base_color, 1.4)
                } else if is_dominant {
                    Self::brighten_color(base_color, 1.15)
                } else {
                    base_color
                };

                // Draw bar with rounded top corners and gradient
                let rounding = egui::Rounding {
                    nw: 3.0,
                    ne: 3.0,
                    sw: 0.0,
                    se: 0.0,
                };

                // Draw gradient: darker at bottom, lighter at top
                let dark_color = Self::darken_color(bar_color, 0.6);

                // Draw bottom half (darker)
                let bottom_half = egui::Rect::from_min_max(
                    egui::pos2(bar_rect.left(), bar_rect.center().y),
                    bar_rect.max,
                );
                if bottom_half.height() > 0.0 {
                    chart_painter.rect_filled(
                        bottom_half,
                        egui::Rounding { nw: 0.0, ne: 0.0, sw: 0.0, se: 0.0 },
                        dark_color,
                    );
                }

                // Draw top half (lighter)
                let top_half = egui::Rect::from_min_max(
                    bar_rect.min,
                    egui::pos2(bar_rect.right(), bar_rect.center().y),
                );
                if top_half.height() > 0.0 {
                    chart_painter.rect_filled(top_half, rounding, bar_color);
                }

                // Draw value label
                let label_text = if self.relative {
                    format!("{:.0}%", val)
                } else {
                    format!("{:.1}", val)
                };

                if bar_height > 30.0 && bar_width > 12.0 {
                    // Label inside tall bars
                    chart_painter.text(
                        egui::pos2(bar_x + bar_width / 2.0, chart_bottom - bar_height + 12.0),
                        egui::Align2::CENTER_CENTER,
                        &label_text,
                        egui::FontId::proportional(8.0),
                        egui::Color32::from_gray(240),
                    );
                } else if bar_width > 12.0 {
                    // Label above short bars
                    chart_painter.text(
                        egui::pos2(bar_x + bar_width / 2.0, chart_bottom - bar_height - 2.0),
                        egui::Align2::CENTER_BOTTOM,
                        &label_text,
                        egui::FontId::proportional(8.0),
                        egui::Color32::from_gray(180),
                    );
                }

                // Show tooltip on hover
                if is_hovered {
                    egui::show_tooltip_at_pointer(ui.ctx(), egui::Id::new(format!("bar_tip_{}_{}", ch, b)), |ui| {
                        ui.label(egui::RichText::new(format!("{} ({:.1}-{:.1} Hz)", name, f_lo, f_hi)).strong());
                        ui.label(format!("Channel: {}", CHANNEL_NAMES.get(ch).unwrap_or(&"?")));
                        if self.relative {
                            ui.label(format!("Relative Power: {:.1}%", val));
                        } else {
                            ui.label(format!("Absolute Power: {:.2} uV^2", val));
                        }
                    });
                }
            }
        }

        // --- Sparklines Area ---
        ui.add_space(4.0);
        let (sparkline_rect, _) = ui.allocate_exact_size(
            egui::vec2(available.x, sparkline_height),
            egui::Sense::hover(),
        );

        let sparkline_painter = ui.painter_at(sparkline_rect);
        sparkline_painter.rect_filled(sparkline_rect, 2.0, egui::Color32::from_gray(15));

        // Draw label
        sparkline_painter.text(
            egui::pos2(sparkline_rect.left() + 4.0, sparkline_rect.top() + 2.0),
            egui::Align2::LEFT_TOP,
            "Trend (60s)",
            egui::FontId::proportional(8.0),
            egui::Color32::from_gray(100),
        );

        // Draw sparklines for each visible band
        let sparkline_area = egui::Rect::from_min_max(
            egui::pos2(sparkline_rect.left() + 50.0, sparkline_rect.top() + 4.0),
            egui::pos2(sparkline_rect.right() - 4.0, sparkline_rect.bottom() - 4.0),
        );

        for (b, &(_, _, _, color)) in BANDS.iter().enumerate() {
            if self.band_visible[b] {
                self.draw_sparkline(&sparkline_painter, sparkline_area, b, color);
            }
        }

        // Request repaint for animation
        ui.ctx().request_repaint();
    }
}
