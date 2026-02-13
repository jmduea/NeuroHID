//! # Signal Quality Widget
//!
//! Displays per-channel signal quality indicators, RMS amplitude,
//! and railed-sample percentages to help users optimize electrode placement.

use crate::widgets::{Widget, WidgetContext, WidgetId};
use eframe::egui;

/// TODO: also in FFT plot — unify/dynamically generate based on stream metadata?
const CHANNEL_NAMES: &[&str] = &["AF3", "AF4", "T7", "T8", "Pz"];
/// TODO: also in FFT plot — unify/dynamically generate based on stream metadata?
const CHANNEL_COLORS: &[egui::Color32] = &[
    egui::Color32::from_rgb(129, 199, 132),
    egui::Color32::from_rgb(100, 181, 246),
    egui::Color32::from_rgb(239, 154, 154),
    egui::Color32::from_rgb(255, 213, 79),
    egui::Color32::from_rgb(206, 147, 216),
];

///TODO: Make configurable, or derive from stream metadata if available. Could also infer from channel names + standard EEG Layouts.
/// Electrode positions on head diagram (normalized 0-100 coordinate space).
/// Top-down view: nose at top, left ear on left.
const ELECTRODE_POSITIONS: &[(usize, f32, f32)] = &[
    (0, 35.0, 25.0), // AF3 - front-left
    (1, 65.0, 25.0), // AF4 - front-right
    (2, 15.0, 50.0), // T7 - left temporal
    (3, 85.0, 50.0), // T8 - right temporal
    (4, 50.0, 70.0), // Pz - parietal center
];

///TODO: Make thresholds configurable, or derive from device metadata if available.
/// Threshold helpers - based on Emotiv Insight expectations.
const RMS_GOOD: f32 = 100.0; // Good if RMS < 100 uV
const RMS_WARN: f32 = 300.0; // Warning if RMS 100-300 uV
const RAILED_GOOD: f32 = 1.0; // < 1% railed
const RAILED_WARN: f32 = 5.0; // 1-5% railed
///TODO: Another configurable
/// Number of recent samples to analyze for quality metrics.
const WINDOW_SAMPLES: usize = 256; // ~2 seconds at 128 Hz

#[derive(Clone)]
struct ChannelMetrics {
    rms: f32,
    peak_to_peak: f32,
    railed_pct: f32,
    quality: Quality,
    mean: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum Quality {
    Good,
    Warning,
    Bad,
}

impl Quality {
    fn color(self) -> egui::Color32 {
        match self {
            Quality::Good => egui::Color32::from_rgb(76, 175, 80),
            Quality::Warning => egui::Color32::from_rgb(255, 193, 7),
            Quality::Bad => egui::Color32::from_rgb(244, 67, 54),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Quality::Good => "Good",
            Quality::Warning => "Fair",
            Quality::Bad => "Poor",
        }
    }

    /// Get pulse frequency multiplier for animation.
    fn pulse_speed(self) -> f32 {
        match self {
            Quality::Good => 0.0,    // No pulse, steady
            Quality::Warning => 2.0, // Slow pulse
            Quality::Bad => 5.0,     // Fast pulse
        }
    }
}

pub struct SignalQualityWidget {
    /// Smoothing factor for metrics.
    smoothing: f32,
    /// Cached channel metrics.
    cached_metrics: Vec<ChannelMetrics>,
    /// Rail threshold in uV (values above this are "railed").
    rail_threshold: f32,
    /// Whether device-reported quality is available.
    has_device_quality: bool,
}

impl Default for SignalQualityWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalQualityWidget {
    pub fn new() -> Self {
        Self {
            smoothing: 0.9,
            cached_metrics: Vec::new(),
            rail_threshold: 8000.0, // 14-bit ADC typical max
            has_device_quality: false,
        }
    }

    /// Try to derive per-channel quality from a device-reported quality stream.
    /// Emotiv publishes quality streams ("EmotivDeviceQuality", "EmotivEEGQuality")
    /// where `Sample.values` contains per-channel quality indicators followed by
    /// aggregate values (battery, overall, sample-rate).
    ///
    /// The values arrive **pre-normalized** to 0.0–1.0 by the Cortex adapter:
    ///   - Indices 0..N-1 : per-sensor quality (0.0 = no contact, 1.0 = excellent)
    ///   - Index N         : battery percent (0–100 as float)
    ///   - Index N+1       : overall quality (0.0–1.0)
    ///   - Index N+2       : sample-rate quality (float)
    ///
    /// Returns `Some(vec)` with one `ChannelMetrics` per EEG channel if quality
    /// data is available, `None` otherwise.
    fn quality_from_device_stream(
        &self,
        ctx: &WidgetContext<'_>,
        num_eeg_channels: usize,
    ) -> Option<Vec<ChannelMetrics>> {
        // Look for a "Quality" stream in the data bus.
        let quality_samples = ctx.samples_for_type("Quality")?;
        let latest = quality_samples.back()?;

        // Need at least per-channel quality values.
        if latest.values.len() < num_eeg_channels {
            return None;
        }

        // Average the last few quality samples for stability.
        let window = 10.min(quality_samples.len());
        let start = quality_samples.len() - window;

        let mut avg_quality = vec![0.0f32; num_eeg_channels];
        for sample in quality_samples.range(start..) {
            for (ch, quality_sum) in avg_quality
                .iter_mut()
                .enumerate()
                .take(num_eeg_channels)
            {
                if let Some(&v) = sample.values.get(ch) {
                    *quality_sum += v;
                }
            }
        }
        for v in avg_quality.iter_mut() {
            *v /= window as f32;
        }

        let metrics = (0..num_eeg_channels)
            .map(|ch| {
                let q = avg_quality[ch]; // 0.0–1.0 (pre-normalized)
                                         // Map 0.0–1.0 quality to Quality enum:
                                         //   >= 0.75 → Good, >= 0.50 → Warning, <0.50 → Bad
                let quality = if q >= 0.75 {
                    Quality::Good
                } else if q >= 0.50 {
                    Quality::Warning
                } else {
                    Quality::Bad
                };
                // Synthesize approximate metrics from the quality value.
                // We don't have real RMS from this stream, so estimate.
                let quality_frac = q.clamp(0.0, 1.0);
                ChannelMetrics {
                    rms: RMS_WARN * (1.0 - quality_frac),
                    peak_to_peak: 0.0,
                    railed_pct: if q < 0.25 { 100.0 } else { 0.0 },
                    quality,
                    mean: 0.0,
                }
            })
            .collect();

        Some(metrics)
    }

    fn compute_metrics(&self, ctx: &WidgetContext<'_>, channel: usize) -> ChannelMetrics {
        let samples = ctx.samples_for(WidgetId::SignalQuality);
        let start = if samples.len() > WINDOW_SAMPLES {
            samples.len() - WINDOW_SAMPLES
        } else {
            0
        };

        let values: Vec<f32> = samples
            .range(start..)
            .filter_map(|s| s.get(channel))
            .collect();

        if values.is_empty() {
            return ChannelMetrics {
                rms: 0.0,
                peak_to_peak: 0.0,
                railed_pct: 0.0,
                quality: Quality::Bad,
                mean: 0.0,
            };
        }

        let n = values.len() as f32;

        // Mean
        let mean: f32 = values.iter().sum::<f32>() / n;

        // RMS (around mean)
        let rms = (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n).sqrt();

        // Peak-to-peak
        let min_val = values.iter().cloned().fold(f32::INFINITY, f32::min);
        let max_val = values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let peak_to_peak = max_val - min_val;

        // Railed percentage
        let railed_count = values
            .iter()
            .filter(|v| v.abs() > self.rail_threshold)
            .count();
        let railed_pct = (railed_count as f32 / n) * 100.0;

        // Compute quality score
        let quality = if rms < RMS_GOOD && railed_pct < RAILED_GOOD {
            Quality::Good
        } else if rms < RMS_WARN && railed_pct < RAILED_WARN {
            Quality::Warning
        } else {
            Quality::Bad
        };

        ChannelMetrics {
            rms,
            peak_to_peak,
            railed_pct,
            quality,
            mean,
        }
    }

    /// Draw animated quality indicator dot.
    fn draw_quality_dot(
        &self,
        painter: &egui::Painter,
        center: egui::Pos2,
        quality: Quality,
        time: f64,
        base_radius: f32,
    ) {
        let color = quality.color();
        let pulse_speed = quality.pulse_speed();

        let radius = if pulse_speed > 0.0 {
            // Pulsing animation: oscillate size
            let phase = (time * pulse_speed as f64).sin() as f32;
            base_radius * (1.0 + 0.3 * phase)
        } else {
            base_radius
        };

        // Draw outer glow
        let glow_color = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 40);
        painter.circle_filled(center, radius + 2.0, glow_color);

        // Draw main dot
        painter.circle_filled(center, radius, color);

        // Draw bright center highlight
        let highlight = egui::Color32::from_rgba_unmultiplied(255, 255, 255, 80);
        painter.circle_filled(
            egui::pos2(center.x - radius * 0.2, center.y - radius * 0.2),
            radius * 0.3,
            highlight,
        );
    }

    /// Draw the head diagram with electrode positions.
    fn draw_head_diagram(&self, ui: &mut egui::Ui, rect: egui::Rect, time: f64) {
        let painter = ui.painter_at(rect);

        // Calculate scaling to fit in rect
        let size = rect.width().min(rect.height());
        let center = rect.center();
        let scale = size / 100.0;

        // Head outline (circle)
        let head_radius = 40.0 * scale;
        painter.circle_stroke(
            center,
            head_radius,
            egui::Stroke::new(2.0, egui::Color32::from_gray(100)),
        );

        // Nose indicator (triangle at top)
        let nose_tip = egui::pos2(center.x, center.y - head_radius - 8.0 * scale);
        let nose_left = egui::pos2(center.x - 6.0 * scale, center.y - head_radius + 2.0 * scale);
        let nose_right = egui::pos2(center.x + 6.0 * scale, center.y - head_radius + 2.0 * scale);
        painter.line_segment(
            [nose_left, nose_tip],
            egui::Stroke::new(2.0, egui::Color32::from_gray(100)),
        );
        painter.line_segment(
            [nose_right, nose_tip],
            egui::Stroke::new(2.0, egui::Color32::from_gray(100)),
        );

        // Ear indicators (arcs on sides)
        // Left ear
        let left_ear_center = egui::pos2(center.x - head_radius - 4.0 * scale, center.y);
        painter.circle_stroke(
            left_ear_center,
            6.0 * scale,
            egui::Stroke::new(1.5, egui::Color32::from_gray(80)),
        );

        // Right ear
        let right_ear_center = egui::pos2(center.x + head_radius + 4.0 * scale, center.y);
        painter.circle_stroke(
            right_ear_center,
            6.0 * scale,
            egui::Stroke::new(1.5, egui::Color32::from_gray(80)),
        );

        // Draw electrodes at their positions
        for &(ch_idx, norm_x, norm_y) in ELECTRODE_POSITIONS {
            if ch_idx >= self.cached_metrics.len() {
                continue;
            }

            // Convert normalized coordinates to screen position
            // norm_x/norm_y are in 0-100 space, centered at 50,50
            let x = center.x + (norm_x - 50.0) * scale;
            let y = center.y + (norm_y - 50.0) * scale;
            let pos = egui::pos2(x, y);

            let quality = self.cached_metrics[ch_idx].quality;

            // Draw electrode dot
            self.draw_quality_dot(&painter, pos, quality, time, 6.0);

            // Draw channel label
            painter.text(
                egui::pos2(pos.x, pos.y + 10.0),
                egui::Align2::CENTER_TOP,
                CHANNEL_NAMES[ch_idx],
                egui::FontId::proportional(8.0),
                egui::Color32::from_gray(150),
            );
        }
    }

    /// Draw improved quality bar with gradient and segment markers.
    fn draw_quality_bar(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        fill_frac: f32,
        quality: Quality,
    ) {
        let bar_height = 14.0;
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.center().y - bar_height / 2.0),
            egui::vec2(rect.width(), bar_height),
        );

        // Background
        painter.rect_filled(bar_rect, 4.0, egui::Color32::from_gray(40));

        // Segment markers at 25%, 50%, 75%
        for pct in [0.25, 0.50, 0.75] {
            let x = bar_rect.left() + bar_rect.width() * pct;
            painter.line_segment(
                [
                    egui::pos2(x, bar_rect.top()),
                    egui::pos2(x, bar_rect.bottom()),
                ],
                egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
            );
        }

        // Fill with gradient
        let fill_width = bar_rect.width() * fill_frac.clamp(0.0, 1.0);
        if fill_width > 0.0 {
            let fill_rect =
                egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_width, bar_height));

            let base_color = quality.color();
            let dark_color = egui::Color32::from_rgb(
                (base_color.r() as f32 * 0.6) as u8,
                (base_color.g() as f32 * 0.6) as u8,
                (base_color.b() as f32 * 0.6) as u8,
            );

            // Draw bottom half (darker)
            let bottom_half = egui::Rect::from_min_max(
                egui::pos2(fill_rect.left(), fill_rect.center().y),
                fill_rect.max,
            );
            painter.rect_filled(
                bottom_half,
                egui::CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: 4,
                    se: 0,
                },
                dark_color,
            );

            // Draw top half (lighter)
            let top_half = egui::Rect::from_min_max(
                fill_rect.min,
                egui::pos2(fill_rect.right(), fill_rect.center().y),
            );
            painter.rect_filled(
                top_half,
                egui::CornerRadius {
                    nw: 4,
                    ne: 0,
                    sw: 0,
                    se: 0,
                },
                base_color,
            );

            // Show percentage text inside bar if wide enough
            if fill_width > 30.0 {
                painter.text(
                    fill_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{:.0}%", fill_frac * 100.0),
                    egui::FontId::proportional(9.0),
                    egui::Color32::from_gray(240),
                );
            }
        }

        // Border
        painter.rect_stroke(
            bar_rect,
            4.0,
            egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
            egui::StrokeKind::Outside,
        );
    }

    /// Get contextual tip based on overall quality.
    fn get_quality_tip(&self) -> &'static str {
        let has_bad = self
            .cached_metrics
            .iter()
            .any(|m| m.quality == Quality::Bad);
        let has_warning = self
            .cached_metrics
            .iter()
            .any(|m| m.quality == Quality::Warning);
        let all_good = self
            .cached_metrics
            .iter()
            .all(|m| m.quality == Quality::Good);

        if all_good {
            "Signal quality is excellent"
        } else if has_bad {
            "Warning: Check electrode contact. Re-wet saline pads if needed."
        } else if has_warning {
            "Tip: Press electrodes firmly against scalp"
        } else {
            ""
        }
    }
}

impl Widget for SignalQualityWidget {
    fn id(&self) -> WidgetId {
        WidgetId::SignalQuality
    }

    fn title(&self) -> &str {
        "Signal Quality"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>, pane_index: usize) {
        // Get animation time
        let time = ui.ctx().input(|i| i.time);

        if ctx.samples_for(WidgetId::SignalQuality).len() < 32 {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Waiting for data...").weak());
            });
            return;
        }

        // Use discovered stream channel count for stability.
        let num_channels = ctx
            .channel_count_for(&["EEG"])
            .unwrap_or_else(|| {
                ctx.samples_for(WidgetId::SignalQuality)
                    .back()
                    .map(|s| s.channel_count())
                    .unwrap_or(5)
            })
            .min(5);

        // Update metrics: prefer device-reported quality stream,
        // fall back to computing from raw EEG values.
        let device_quality = self.quality_from_device_stream(ctx, num_channels);
        self.has_device_quality = device_quality.is_some();
        let new_metrics: Vec<ChannelMetrics> = device_quality.unwrap_or_else(|| {
            (0..num_channels)
                .map(|ch| self.compute_metrics(ctx, ch))
                .collect()
        });

        if self.cached_metrics.len() != num_channels {
            self.cached_metrics = new_metrics;
        } else {
            let s = self.smoothing;
            for (cached, new) in self.cached_metrics.iter_mut().zip(new_metrics.iter()) {
                cached.rms = s * cached.rms + (1.0 - s) * new.rms;
                cached.peak_to_peak = s * cached.peak_to_peak + (1.0 - s) * new.peak_to_peak;
                cached.railed_pct = s * cached.railed_pct + (1.0 - s) * new.railed_pct;
                cached.quality = new.quality;
                cached.mean = s * cached.mean + (1.0 - s) * new.mean;
            }
        }

        // Overall quality summary
        let overall = if self
            .cached_metrics
            .iter()
            .all(|m| m.quality == Quality::Good)
        {
            Quality::Good
        } else if self
            .cached_metrics
            .iter()
            .any(|m| m.quality == Quality::Bad)
        {
            Quality::Bad
        } else {
            Quality::Warning
        };

        // --- Header with animated overall indicator ---
        ui.horizontal(|ui| {
            // Animated overall quality dot
            let (dot_rect, _) =
                ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
            let painter = ui.painter_at(dot_rect);
            self.draw_quality_dot(&painter, dot_rect.center(), overall, time, 6.0);

            ui.label(egui::RichText::new(format!("Overall: {}", overall.label())).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} channels | {} Hz", num_channels, 128))
                        .small()
                        .weak(),
                );
            });
        });

        ui.separator();

        // --- Head Diagram ---
        let head_size = 120.0;
        let (head_rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), head_size),
            egui::Sense::hover(),
        );
        self.draw_head_diagram(ui, head_rect, time);

        ui.add_space(4.0);

        // --- Channel quality table with alternating rows ---
        egui::Grid::new(format!("signal_quality_grid_{}", pane_index))
            .num_columns(6)
            .spacing([12.0, 4.0])
            .min_col_width(40.0)
            .show(ui, |ui| {
                // Header row
                ui.label(egui::RichText::new("Ch").strong());
                ui.label(egui::RichText::new("Status").strong());
                ui.label(egui::RichText::new("RMS (uV)").strong());
                ui.label(egui::RichText::new("P-P (uV)").strong());
                ui.label(egui::RichText::new("Railed %").strong());
                ui.label(egui::RichText::new("DC (uV)").strong());
                ui.end_row();

                for ch in 0..num_channels {
                    let m = &self.cached_metrics[ch];
                    let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];
                    let row_idx = ch;

                    // Alternating row background
                    let row_bg = if row_idx % 2 == 0 {
                        egui::Color32::from_gray(25)
                    } else {
                        egui::Color32::from_gray(30)
                    };

                    // Channel name with background
                    ui.scope(|ui| {
                        let rect = ui.available_rect_before_wrap();
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(
                                rect.min,
                                egui::vec2(ui.available_width(), 18.0),
                            ),
                            0.0,
                            row_bg,
                        );
                        ui.label(egui::RichText::new(CHANNEL_NAMES[ch]).color(color));
                    });

                    // Quality indicator with animated dot
                    ui.horizontal(|ui| {
                        let (dot_rect, _) =
                            ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        let painter = ui.painter_at(dot_rect);
                        self.draw_quality_dot(&painter, dot_rect.center(), m.quality, time, 4.0);
                        ui.label(egui::RichText::new(m.quality.label()).color(m.quality.color()));
                    });

                    // RMS with proportional bar fill
                    ui.scope(|ui| {
                        let available = ui.available_rect_before_wrap();
                        let bar_rect =
                            egui::Rect::from_min_size(available.min, egui::vec2(60.0, 16.0));
                        let painter = ui.painter();

                        // Background bar
                        painter.rect_filled(bar_rect, 2.0, egui::Color32::from_gray(35));

                        // Fill based on RMS (inverted: lower is better)
                        let fill_frac = (m.rms / RMS_WARN).min(1.0);
                        let rms_color = if m.rms < RMS_GOOD {
                            Quality::Good.color()
                        } else if m.rms < RMS_WARN {
                            Quality::Warning.color()
                        } else {
                            Quality::Bad.color()
                        };
                        let fill_rect = egui::Rect::from_min_size(
                            bar_rect.min,
                            egui::vec2(bar_rect.width() * fill_frac, bar_rect.height()),
                        );
                        painter.rect_filled(fill_rect, 2.0, rms_color.gamma_multiply(0.4));

                        // Text on top
                        painter.text(
                            bar_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("{:.1}", m.rms),
                            egui::FontId::proportional(10.0),
                            egui::Color32::from_gray(220),
                        );

                        ui.allocate_exact_size(egui::vec2(60.0, 16.0), egui::Sense::hover());
                    });

                    // Peak-to-peak
                    ui.label(format!("{:.0}", m.peak_to_peak));

                    // Railed % with bar fill
                    ui.scope(|ui| {
                        let available = ui.available_rect_before_wrap();
                        let bar_rect =
                            egui::Rect::from_min_size(available.min, egui::vec2(55.0, 16.0));
                        let painter = ui.painter();

                        painter.rect_filled(bar_rect, 2.0, egui::Color32::from_gray(35));

                        let fill_frac = (m.railed_pct / 10.0).min(1.0);
                        let railed_color = if m.railed_pct < RAILED_GOOD {
                            Quality::Good.color()
                        } else if m.railed_pct < RAILED_WARN {
                            Quality::Warning.color()
                        } else {
                            Quality::Bad.color()
                        };
                        let fill_rect = egui::Rect::from_min_size(
                            bar_rect.min,
                            egui::vec2(bar_rect.width() * fill_frac, bar_rect.height()),
                        );
                        painter.rect_filled(fill_rect, 2.0, railed_color.gamma_multiply(0.4));

                        painter.text(
                            bar_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("{:.2}%", m.railed_pct),
                            egui::FontId::proportional(10.0),
                            egui::Color32::from_gray(220),
                        );

                        ui.allocate_exact_size(egui::vec2(55.0, 16.0), egui::Sense::hover());
                    });

                    // DC offset (mean)
                    ui.label(format!("{:.1}", m.mean));

                    ui.end_row();
                }
            });

        // --- Visual quality bars at the bottom ---
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Quality Overview").small().weak());

        for ch in 0..num_channels {
            let m = &self.cached_metrics[ch];
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(CHANNEL_NAMES[ch])
                        .color(CHANNEL_COLORS[ch % CHANNEL_COLORS.len()])
                        .monospace(),
                );

                let bar_width = (ui.available_width() - 10.0).max(60.0);
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(bar_width, 18.0), egui::Sense::hover());

                // Fill based on inverse RMS (lower = better = longer bar)
                let fill_frac = (1.0 - (m.rms / RMS_WARN).min(1.0)).max(0.0);
                let painter = ui.painter_at(rect);
                self.draw_quality_bar(&painter, rect, fill_frac, m.quality);
            });
        }

        // --- Contextual tip ---
        ui.add_space(8.0);
        let tip = self.get_quality_tip();
        if !tip.is_empty() {
            let tip_color = match overall {
                Quality::Good => egui::Color32::from_rgb(76, 175, 80),
                Quality::Warning => egui::Color32::from_rgb(255, 193, 7),
                Quality::Bad => egui::Color32::from_rgb(244, 67, 54),
            };

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("*").color(tip_color));
                ui.label(egui::RichText::new(tip).color(tip_color).italics());
            });
        }

        // Request repaint for animations
        ui.ctx().request_repaint();
    }
}
