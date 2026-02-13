//! # FFT Plot Widget
//!
//! Displays the power spectral density of the EEG signal in real time.
//! X-axis is frequency (0-64 Hz), Y-axis is amplitude in uV.

use crate::widgets::{Widget, WidgetContext, WidgetId};
use eframe::egui;
use std::collections::VecDeque;

///TODO: Dynamic colors based on theme, channel count
const CHANNEL_COLORS: &[egui::Color32] = &[
    egui::Color32::from_rgb(129, 199, 132),
    egui::Color32::from_rgb(100, 181, 246),
    egui::Color32::from_rgb(239, 154, 154),
    egui::Color32::from_rgb(255, 213, 79),
    egui::Color32::from_rgb(206, 147, 216),
];

///TODO: Dynamic channel names based on stream metadata
const CHANNEL_NAMES: &[&str] = &["AF3", "AF4", "T7", "T8", "Pz"];

/// Number of FFT bins (power of 2).
const FFT_SIZE: usize = 256;

/// Left margin for Y-axis labels
const LEFT_MARGIN: f32 = 50.0;

/// EEG frequency bands with their characteristics (name, tooltip, f_lo, f_hi, r, g, b, a)
const BANDS: &[(&str, &str, f32, f32, u8, u8, u8, u8)] = &[
    (
        "delta",
        "Delta (0.5-4 Hz): Deep sleep, unconscious",
        0.5,
        4.0,
        100,
        100,
        200,
        25,
    ),
    (
        "theta",
        "Theta (4-8 Hz): Drowsiness, light sleep, meditation",
        4.0,
        8.0,
        100,
        200,
        100,
        25,
    ),
    (
        "alpha",
        "Alpha (8-13 Hz): Relaxed, calm, eyes closed",
        8.0,
        13.0,
        200,
        200,
        100,
        25,
    ),
    (
        "beta",
        "Beta (13-30 Hz): Alert, active thinking, focus",
        13.0,
        30.0,
        200,
        100,
        100,
        25,
    ),
    (
        "gamma",
        "Gamma (30-45 Hz): High-level cognition, perception",
        30.0,
        45.0,
        200,
        100,
        200,
        25,
    ),
];

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
    /// Whether FFT updates are frozen.
    frozen: bool,
    /// Optional bound source stream id.
    selected_source: Option<String>,
}

impl FftPlotWidget {
    pub fn new() -> Self {
        Self {
            log_scale: true,
            smoothing: 0.7,
            cached_fft: Vec::new(),
            channel_enabled: [true; 5],
            max_freq: 60.0,
            frozen: false,
            selected_source: None,
        }
    }

    /// Compute FFT magnitude spectrum for a single channel from recent samples.
    fn compute_fft(samples: &VecDeque<neurohid_types::signal::Sample>, channel: usize) -> Vec<f32> {
        let n = FFT_SIZE;
        // Gather the most recent n values for this channel
        let start = if samples.len() > n {
            samples.len() - n
        } else {
            0
        };
        let mut real: Vec<f32> = samples
            .range(start..)
            .filter_map(|s| s.get(channel))
            .collect();

        if real.len() < n {
            // Not enough data - pad with zeros
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

    /// Find peak frequency for a channel's FFT data.
    /// Returns (bin_index, frequency, magnitude) if peak is significant.
    fn find_peak(
        fft: &[f32],
        bins_to_show: usize,
        nyquist: f32,
        half_bins: usize,
    ) -> Option<(usize, f32, f32)> {
        if fft.is_empty() || bins_to_show == 0 {
            return None;
        }

        // Find median for noise floor estimation
        let mut sorted: Vec<f32> = fft.iter().take(bins_to_show).copied().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);

        // Find peak
        let mut max_idx = 0;
        let mut max_val = 0.0f32;
        for (i, &v) in fft.iter().take(bins_to_show).enumerate() {
            if v > max_val {
                max_val = v;
                max_idx = i;
            }
        }

        // Only return if peak is significantly above noise floor (>2x median)
        if max_val > median * 2.0 && max_val > 1e-8 {
            let freq = max_idx as f32 * (nyquist / half_bins as f32);
            Some((max_idx, freq, max_val))
        } else {
            None
        }
    }

    /// Get the band name for a given frequency.
    fn get_band_name(freq: f32) -> &'static str {
        for &(name, _, f_lo, f_hi, _, _, _, _) in BANDS {
            if freq >= f_lo && freq < f_hi {
                return name;
            }
        }
        "other"
    }

    /// Draw Y-axis labels and grid lines.
    fn draw_y_axis(
        painter: &egui::Painter,
        rect: egui::Rect,
        y_max: f32,
        log_scale: bool,
        _left_margin: f32,
    ) {
        let axis_x = rect.left() - 5.0;
        let grid_color = egui::Color32::from_gray(35);
        let label_color = egui::Color32::from_gray(120);

        if log_scale {
            // Log scale: label decades
            let log_max = y_max.log10();
            let log_min = log_max - 4.0;

            for decade in -6..=0i32 {
                let log_val = decade as f32;
                if log_val < log_min || log_val > log_max {
                    continue;
                }

                let y_norm = (log_val - log_min) / (log_max - log_min);
                let y = rect.bottom() - y_norm * rect.height();

                // Grid line
                painter.line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    egui::Stroke::new(0.5, grid_color),
                );

                // Label
                let label = format!("1e{}", decade);
                painter.text(
                    egui::pos2(axis_x, y),
                    egui::Align2::RIGHT_CENTER,
                    label,
                    egui::FontId::proportional(8.0),
                    label_color,
                );
            }
        } else {
            // Linear scale: label at 25%, 50%, 75%, 100%
            for pct in &[0.25f32, 0.5, 0.75, 1.0] {
                let y = rect.bottom() - pct * rect.height();
                let val = pct * y_max;

                // Grid line
                painter.line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    egui::Stroke::new(0.5, grid_color),
                );

                // Label
                let label = if val < 0.01 {
                    format!("{:.1e}", val)
                } else {
                    format!("{:.2}", val)
                };
                painter.text(
                    egui::pos2(axis_x, y),
                    egui::Align2::RIGHT_CENTER,
                    label,
                    egui::FontId::proportional(8.0),
                    label_color,
                );
            }
        }
    }

    /// Draw band shading with improved labels.
    fn draw_band_shading(
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        rect: egui::Rect,
        freq_range: f32,
        hover_pos: Option<egui::Pos2>,
        pane_index: usize,
    ) {
        let greek_labels: std::collections::HashMap<&str, &str> = [
            ("delta", "d"),
            ("theta", "0"),
            ("alpha", "a"),
            ("beta", "B"),
            ("gamma", "y"),
        ]
        .into_iter()
        .collect();

        for &(name, tooltip, f_lo, f_hi, r, g, b, a) in BANDS {
            if f_lo > freq_range {
                continue;
            }

            let fill = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
            let x0 = rect.left() + (f_lo / freq_range) * rect.width();
            let x1 = rect.left() + (f_hi.min(freq_range) / freq_range) * rect.width();

            let band_rect =
                egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom()));

            // Draw shading
            painter.rect_filled(band_rect, 0.0, fill);

            // Draw label at top with Greek letter
            let label = greek_labels.get(name).unwrap_or(&"?");
            let label_x = (x0 + x1) / 2.0;

            // Background for label
            let label_bg = egui::Rect::from_center_size(
                egui::pos2(label_x, rect.top() + 10.0),
                egui::vec2(16.0, 14.0),
            );
            painter.rect_filled(
                label_bg,
                2.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 150),
            );

            painter.text(
                egui::pos2(label_x, rect.top() + 10.0),
                egui::Align2::CENTER_CENTER,
                *label,
                egui::FontId::proportional(11.0),
                egui::Color32::from_gray(200),
            );

            // Show tooltip if hovering over this band
            if let Some(pos) = hover_pos {
                if band_rect.contains(pos) {
                    let _ = egui::Tooltip::always_open(
                        ui.ctx().clone(),
                        ui.layer_id(),
                        egui::Id::new(format!("fft_band_{}_{}", pane_index, name)),
                        egui::PopupAnchor::Pointer,
                    )
                    .gap(12.0)
                    .show(|ui| {
                        ui.label(tooltip);
                    });
                }
            }
        }
    }

    /// Draw crosshair with frequency and power readout.
    fn draw_crosshair(
        &self,
        _ui: &mut egui::Ui,
        painter: &egui::Painter,
        rect: egui::Rect,
        hover_pos: egui::Pos2,
        freq_range: f32,
        nyquist: f32,
        half_bins: usize,
        bins_to_show: usize,
        y_max: f32,
        num_channels: usize,
    ) {
        if !rect.contains(hover_pos) {
            return;
        }

        // Vertical crosshair line
        painter.line_segment(
            [
                egui::pos2(hover_pos.x, rect.top()),
                egui::pos2(hover_pos.x, rect.bottom()),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(150)),
        );

        // Calculate frequency at crosshair
        let x_ratio = (hover_pos.x - rect.left()) / rect.width();
        let freq = x_ratio * freq_range;
        let bin_idx = ((freq / nyquist) * half_bins as f32) as usize;
        let bin_idx = bin_idx.min(bins_to_show.saturating_sub(1));

        // Build tooltip content
        let mut tooltip_lines: Vec<(String, egui::Color32)> = Vec::new();
        tooltip_lines.push((format!("f = {:.1} Hz", freq), egui::Color32::WHITE));

        // Add power values for each enabled channel
        for ch in 0..num_channels {
            if !self.channel_enabled[ch] {
                continue;
            }

            if let Some(fft) = self.cached_fft.get(ch) {
                if let Some(&power) = fft.get(bin_idx) {
                    let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];
                    let name = CHANNEL_NAMES.get(ch).unwrap_or(&"?");
                    let power_str = if power < 0.001 {
                        format!("{}: {:.2e} uV", name, power)
                    } else {
                        format!("{}: {:.4} uV", name, power)
                    };
                    tooltip_lines.push((power_str, color));

                    // Draw marker on the line at this frequency
                    let y_norm = if self.log_scale {
                        let log_val = power.max(1e-10).log10();
                        let log_max = y_max.log10();
                        let log_min = log_max - 4.0;
                        ((log_val - log_min) / (log_max - log_min)).clamp(0.0, 1.0)
                    } else {
                        (power / y_max).clamp(0.0, 1.0)
                    };
                    let y = rect.bottom() - y_norm * rect.height();

                    painter.circle_filled(egui::pos2(hover_pos.x, y), 3.0, color);
                    painter.circle_stroke(
                        egui::pos2(hover_pos.x, y),
                        3.0,
                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                    );
                }
            }
        }

        // Draw tooltip near cursor
        let tooltip_x = hover_pos.x + 15.0;
        let tooltip_y = hover_pos.y;

        // Calculate tooltip size
        let line_height = 14.0;
        let tooltip_height = tooltip_lines.len() as f32 * line_height + 8.0;
        let tooltip_width = 120.0;

        // Adjust position if too close to edge
        let tooltip_x = if tooltip_x + tooltip_width > rect.right() {
            hover_pos.x - tooltip_width - 10.0
        } else {
            tooltip_x
        };

        let tooltip_rect = egui::Rect::from_min_size(
            egui::pos2(tooltip_x, tooltip_y - tooltip_height / 2.0),
            egui::vec2(tooltip_width, tooltip_height),
        );

        painter.rect_filled(
            tooltip_rect,
            4.0,
            egui::Color32::from_rgba_unmultiplied(30, 30, 30, 230),
        );
        painter.rect_stroke(
            tooltip_rect,
            4.0,
            egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
            egui::StrokeKind::Outside,
        );

        for (i, (text, color)) in tooltip_lines.iter().enumerate() {
            painter.text(
                egui::pos2(
                    tooltip_rect.left() + 6.0,
                    tooltip_rect.top() + 4.0 + i as f32 * line_height,
                ),
                egui::Align2::LEFT_TOP,
                text,
                egui::FontId::proportional(10.0),
                *color,
            );
        }
    }

    /// Draw peak markers and labels for each channel.
    fn draw_peaks(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        freq_range: f32,
        nyquist: f32,
        half_bins: usize,
        bins_to_show: usize,
        y_max: f32,
        num_channels: usize,
    ) {
        for ch in 0..num_channels {
            if !self.channel_enabled[ch] {
                continue;
            }

            if let Some(fft) = self.cached_fft.get(ch) {
                if let Some((_, freq, mag)) = Self::find_peak(fft, bins_to_show, nyquist, half_bins)
                {
                    let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];

                    // Calculate position
                    let x = rect.left() + (freq / freq_range) * rect.width();

                    let y_norm = if self.log_scale {
                        let log_val = mag.max(1e-10).log10();
                        let log_max = y_max.log10();
                        let log_min = log_max - 4.0;
                        ((log_val - log_min) / (log_max - log_min)).clamp(0.0, 1.0)
                    } else {
                        (mag / y_max).clamp(0.0, 1.0)
                    };
                    let y = rect.bottom() - y_norm * rect.height();

                    // Draw peak marker (filled circle)
                    painter.circle_filled(egui::pos2(x, y), 4.0, color);
                    painter.circle_stroke(
                        egui::pos2(x, y),
                        4.0,
                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                    );

                    // Draw peak label
                    let label = format!("{:.1} Hz", freq);
                    let label_y = (y - 12.0).max(rect.top() + 20.0);

                    // Background for label
                    let galley = painter.layout_no_wrap(
                        label.clone(),
                        egui::FontId::proportional(9.0),
                        color,
                    );
                    let label_rect = egui::Rect::from_min_size(
                        egui::pos2(x - galley.size().x / 2.0 - 3.0, label_y - 1.0),
                        galley.size() + egui::vec2(6.0, 2.0),
                    );
                    painter.rect_filled(
                        label_rect,
                        2.0,
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180),
                    );

                    painter.text(
                        egui::pos2(x, label_y),
                        egui::Align2::CENTER_TOP,
                        label,
                        egui::FontId::proportional(9.0),
                        color,
                    );
                }
            }
        }
    }
}

impl Widget for FftPlotWidget {
    fn id(&self) -> WidgetId {
        WidgetId::FftPlot
    }

    fn title(&self) -> &str {
        "FFT Plot"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>, pane_index: usize) {
        // TODO: Get actual sample rate from stream metadata
        let sample_rate = 128.0;
        let nyquist = sample_rate / 2.0;
        let half_bins = FFT_SIZE / 2;
        let source_options = ctx.candidate_sources_for(WidgetId::FftPlot);
        if !source_options.is_empty() {
            let valid = self
                .selected_source
                .as_ref()
                .map(|id| source_options.iter().any(|s| s.id == *id))
                .unwrap_or(false);
            if !valid {
                self.selected_source = Some(source_options[0].id.clone());
                self.cached_fft.clear();
            }
        }

        // Settings bar
        ui.horizontal(|ui| {
            // Freeze toggle
            let freeze_text = if self.frozen { "Unfreeze" } else { "Freeze" };
            let freeze_color = if self.frozen {
                egui::Color32::from_rgb(100, 181, 246)
            } else {
                ui.visuals().widgets.inactive.fg_stroke.color
            };
            if ui
                .button(egui::RichText::new(freeze_text).color(freeze_color))
                .clicked()
            {
                self.frozen = !self.frozen;
            }

            ui.separator();

            ui.label("Y-Axis:");
            egui::ComboBox::from_id_salt(format!("fft_scale_{}", pane_index))
                .selected_text(if self.log_scale { "Log" } else { "Linear" })
                .width(60.0)
                .show_ui(ui, |ui: &mut egui::Ui| {
                    ui.selectable_value(&mut self.log_scale, true, "Log");
                    ui.selectable_value(&mut self.log_scale, false, "Linear");
                });

            ui.label("Smooth:");
            ui.add(egui::Slider::new(&mut self.smoothing, 0.0..=0.95).max_decimals(2));

            ui.label("Max Hz:");
            egui::ComboBox::from_id_salt(format!("fft_maxfreq_{}", pane_index))
                .selected_text(format!("{:.0}", self.max_freq))
                .width(50.0)
                .show_ui(ui, |ui: &mut egui::Ui| {
                    for &v in &[30.0, 45.0, 60.0, 64.0f32] {
                        ui.selectable_value(&mut self.max_freq, v, format!("{:.0}", v));
                    }
                });

            if !source_options.is_empty() {
                ui.label("Source:");
                let prev = self.selected_source.clone();
                egui::ComboBox::from_id_salt(format!("fft_src_{}", pane_index))
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
                if self.selected_source != prev {
                    self.cached_fft.clear();
                }
            }

            // Channel toggles
            ui.separator();
            for ch in 0..5 {
                let color = CHANNEL_COLORS[ch];
                let mut enabled = self.channel_enabled[ch];
                if ui
                    .checkbox(
                        &mut enabled,
                        egui::RichText::new(CHANNEL_NAMES[ch]).color(color).small(),
                    )
                    .changed()
                {
                    self.channel_enabled[ch] = enabled;
                }
            }

            ui.separator();

            // Show dominant frequency for first enabled channel
            let first_enabled = (0..5).find(|&ch| self.channel_enabled[ch]);
            if let Some(ch) = first_enabled {
                let freq_range = self.max_freq.min(nyquist);
                let bins_to_show = ((freq_range / nyquist) * half_bins as f32) as usize;

                if let Some(fft) = self.cached_fft.get(ch) {
                    if let Some((_, freq, _)) =
                        Self::find_peak(fft, bins_to_show, nyquist, half_bins)
                    {
                        let band = Self::get_band_name(freq);
                        let band_display = match band {
                            "delta" => "Delta",
                            "theta" => "Theta",
                            "alpha" => "Alpha",
                            "beta" => "Beta",
                            "gamma" => "Gamma",
                            _ => "Other",
                        };
                        ui.label(
                            egui::RichText::new(format!(
                                "Dominant: {:.1} Hz ({})",
                                freq, band_display
                            ))
                            .weak()
                            .small(),
                        );
                    }
                }
            }
        });

        let eeg_samples =
            ctx.samples_for_widget_source(WidgetId::FftPlot, self.selected_source.as_deref());
        if eeg_samples.len() < 64 {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Collecting data...").weak());
            });
            return;
        }

        // Compute FFT for each channel (unless frozen)
        // Use discovered stream channel count for stability —
        // avoids cache thrashing when mixed streams cause count to flicker.
        let num_channels = ctx
            .channel_count_for_source(self.selected_source.as_deref().unwrap_or_default())
            .or_else(|| ctx.channel_count_for(&["EEG"]))
            .unwrap_or_else(|| eeg_samples.back().map(|s| s.channel_count()).unwrap_or(5))
            .min(5);

        if !self.frozen {
            // Update cached FFT with smoothing
            if self.cached_fft.len() != num_channels {
                self.cached_fft = (0..num_channels)
                    .map(|ch| Self::compute_fft(eeg_samples, ch))
                    .collect();
            } else {
                for ch in 0..num_channels {
                    let new_fft = Self::compute_fft(eeg_samples, ch);
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
        }

        // Draw the FFT plot
        let available = ui.available_size();
        let (total_rect, response) = ui.allocate_exact_size(available, egui::Sense::hover());

        if !ui.is_rect_visible(total_rect) {
            return;
        }

        // Plot area (with left margin for Y-axis)
        let plot_rect = egui::Rect::from_min_max(
            egui::pos2(total_rect.left() + LEFT_MARGIN, total_rect.top()),
            egui::pos2(total_rect.right(), total_rect.bottom() - 16.0),
        );

        let painter = ui.painter_at(total_rect);
        painter.rect_filled(plot_rect, 0.0, egui::Color32::from_gray(20));

        let freq_range = self.max_freq.min(nyquist);
        let bins_to_show = ((freq_range / nyquist) * half_bins as f32) as usize;

        // Find global max for y-axis scaling
        let mut y_max = 1.0e-6f32;
        for ch in 0..num_channels {
            if !self.channel_enabled[ch] {
                continue;
            }
            if let Some(fft) = self.cached_fft.get(ch) {
                for &v in fft.iter().take(bins_to_show) {
                    y_max = y_max.max(v);
                }
            }
        }

        // Draw Y-axis labels and grid
        Self::draw_y_axis(&painter, plot_rect, y_max, self.log_scale, LEFT_MARGIN);

        // Draw band shading with tooltips
        let hover_pos = response.hover_pos();
        Self::draw_band_shading(ui, &painter, plot_rect, freq_range, hover_pos, pane_index);

        // Draw frequency grid lines
        let freq_ticks = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0f32];
        for &freq in &freq_ticks {
            if freq > freq_range {
                break;
            }
            let x = plot_rect.left() + (freq / freq_range) * plot_rect.width();
            painter.line_segment(
                [
                    egui::pos2(x, plot_rect.top()),
                    egui::pos2(x, plot_rect.bottom()),
                ],
                egui::Stroke::new(0.5, egui::Color32::from_gray(40)),
            );
            painter.text(
                egui::pos2(x, plot_rect.bottom() + 2.0),
                egui::Align2::CENTER_TOP,
                format!("{:.0}", freq),
                egui::FontId::proportional(9.0),
                egui::Color32::from_gray(120),
            );
        }

        // Draw channel FFT lines
        for ch in 0..num_channels {
            if !self.channel_enabled[ch] {
                continue;
            }
            let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];

            if let Some(fft) = self.cached_fft.get(ch) {
                let points: Vec<egui::Pos2> = (0..bins_to_show)
                    .map(|i| {
                        let freq = i as f32 * (nyquist / half_bins as f32);
                        let x = plot_rect.left() + (freq / freq_range) * plot_rect.width();
                        let val = fft.get(i).copied().unwrap_or(0.0).max(1e-10);
                        let y_norm = if self.log_scale {
                            let log_val = val.log10();
                            let log_max = y_max.log10();
                            let log_min = log_max - 4.0;
                            ((log_val - log_min) / (log_max - log_min)).clamp(0.0, 1.0)
                        } else {
                            (val / y_max).clamp(0.0, 1.0)
                        };
                        let y = plot_rect.bottom() - y_norm * plot_rect.height();
                        egui::pos2(x, y)
                    })
                    .collect();

                if points.len() >= 2 {
                    painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, color)));
                }
            }
        }

        // Draw peaks
        self.draw_peaks(
            &painter,
            plot_rect,
            freq_range,
            nyquist,
            half_bins,
            bins_to_show,
            y_max,
            num_channels,
        );

        // Draw crosshair if hovering
        if let Some(pos) = hover_pos {
            self.draw_crosshair(
                ui,
                &painter,
                plot_rect,
                pos,
                freq_range,
                nyquist,
                half_bins,
                bins_to_show,
                y_max,
                num_channels,
            );
        }

        // Axis labels
        painter.text(
            egui::pos2(plot_rect.center().x, total_rect.bottom() - 2.0),
            egui::Align2::CENTER_BOTTOM,
            "Frequency (Hz)",
            egui::FontId::proportional(10.0),
            egui::Color32::from_gray(100),
        );
    }
}
