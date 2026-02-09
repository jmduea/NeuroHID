//! # Signal Quality Widget
//!
//! Displays per-channel signal quality indicators, RMS amplitude,
//! and railed-sample percentages to help users optimize electrode placement.

use eframe::egui;
use crate::widgets::{Widget, WidgetContext, WidgetId};

const CHANNEL_NAMES: &[&str] = &["AF3", "AF4", "T7", "T8", "Pz"];

const CHANNEL_COLORS: &[egui::Color32] = &[
    egui::Color32::from_rgb(129, 199, 132),
    egui::Color32::from_rgb(100, 181, 246),
    egui::Color32::from_rgb(239, 154, 154),
    egui::Color32::from_rgb(255, 213, 79),
    egui::Color32::from_rgb(206, 147, 216),
];

/// Threshold helpers — based on Emotiv Insight expectations.
const RMS_GOOD: f32 = 100.0;     // Good if RMS < 100 µV
const RMS_WARN: f32 = 300.0;     // Warning if RMS 100–300 µV
const RAILED_GOOD: f32 = 1.0;    // < 1% railed
const RAILED_WARN: f32 = 5.0;    // 1–5% railed

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

    fn icon(self) -> &'static str {
        match self {
            Quality::Good => "●",
            Quality::Warning => "●",
            Quality::Bad => "●",
        }
    }
}

pub struct SignalQualityWidget {
    /// Smoothing factor for metrics.
    smoothing: f32,
    /// Cached channel metrics.
    cached_metrics: Vec<ChannelMetrics>,
    /// Rail threshold in µV (values above this are "railed").
    rail_threshold: f32,
}

impl SignalQualityWidget {
    pub fn new() -> Self {
        Self {
            smoothing: 0.9,
            cached_metrics: Vec::new(),
            rail_threshold: 8000.0, // 14-bit ADC typical max
        }
    }

    fn compute_metrics(
        &self,
        ctx: &WidgetContext<'_>,
        channel: usize,
    ) -> ChannelMetrics {
        let samples = &ctx.bus.samples;
        let start = if samples.len() > WINDOW_SAMPLES {
            samples.len() - WINDOW_SAMPLES
        } else {
            0
        };

        let values: Vec<f32> = samples.range(start..)
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
        let railed_count = values.iter()
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
}

impl Widget for SignalQualityWidget {
    fn id(&self) -> WidgetId {
        WidgetId::SignalQuality
    }

    fn title(&self) -> &str {
        "Signal Quality"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>) {
        if ctx.bus.samples.len() < 32 {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Waiting for data...").weak());
            });
            return;
        }

        let num_channels = ctx.bus.samples.back()
            .map(|s| s.channel_count())
            .unwrap_or(5)
            .min(5);

        // Update metrics with smoothing
        let new_metrics: Vec<ChannelMetrics> = (0..num_channels)
            .map(|ch| self.compute_metrics(ctx, ch))
            .collect();

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
        let overall = if self.cached_metrics.iter().all(|m| m.quality == Quality::Good) {
            Quality::Good
        } else if self.cached_metrics.iter().any(|m| m.quality == Quality::Bad) {
            Quality::Bad
        } else {
            Quality::Warning
        };

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(overall.icon()).color(overall.color()).size(16.0));
            ui.label(egui::RichText::new(format!("Overall: {}", overall.label())).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} channels • {} Hz", num_channels, 128))
                        .small()
                        .weak(),
                );
            });
        });

        ui.separator();

        // Channel quality table
        egui::Grid::new("signal_quality_grid")
            .num_columns(6)
            .spacing([12.0, 6.0])
            .striped(true)
            .show(ui, |ui| {
                // Header
                ui.label(egui::RichText::new("Ch").strong());
                ui.label(egui::RichText::new("Status").strong());
                ui.label(egui::RichText::new("RMS (µV)").strong());
                ui.label(egui::RichText::new("P-P (µV)").strong());
                ui.label(egui::RichText::new("Railed %").strong());
                ui.label(egui::RichText::new("DC (µV)").strong());
                ui.end_row();

                for ch in 0..num_channels {
                    let m = &self.cached_metrics[ch];
                    let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];

                    // Channel name
                    ui.label(egui::RichText::new(CHANNEL_NAMES[ch]).color(color));

                    // Quality indicator
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(m.quality.icon()).color(m.quality.color()));
                        ui.label(egui::RichText::new(m.quality.label()).color(m.quality.color()));
                    });

                    // RMS
                    let rms_color = if m.rms < RMS_GOOD {
                        egui::Color32::from_gray(200)
                    } else if m.rms < RMS_WARN {
                        egui::Color32::from_rgb(255, 193, 7)
                    } else {
                        egui::Color32::from_rgb(244, 67, 54)
                    };
                    ui.label(egui::RichText::new(format!("{:.1}", m.rms)).color(rms_color));

                    // Peak-to-peak
                    ui.label(format!("{:.0}", m.peak_to_peak));

                    // Railed %
                    let railed_color = if m.railed_pct < RAILED_GOOD {
                        egui::Color32::from_gray(200)
                    } else if m.railed_pct < RAILED_WARN {
                        egui::Color32::from_rgb(255, 193, 7)
                    } else {
                        egui::Color32::from_rgb(244, 67, 54)
                    };
                    ui.label(egui::RichText::new(format!("{:.2}%", m.railed_pct)).color(railed_color));

                    // DC offset (mean)
                    ui.label(format!("{:.1}", m.mean));

                    ui.end_row();
                }
            });

        // Visual quality bars at the bottom
        ui.add_space(8.0);
        let available_width = ui.available_width();

        for ch in 0..num_channels {
            let m = &self.cached_metrics[ch];
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(CHANNEL_NAMES[ch])
                    .color(CHANNEL_COLORS[ch % CHANNEL_COLORS.len()])
                    .monospace());

                let bar_width = (available_width - 60.0).max(40.0);
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(bar_width, 10.0),
                    egui::Sense::hover(),
                );

                let painter = ui.painter_at(rect);

                // Background
                painter.rect_filled(rect, 4.0, egui::Color32::from_gray(40));

                // Fill based on inverse RMS (lower = better = longer bar)
                let fill_frac = (1.0 - (m.rms / RMS_WARN).min(1.0)).max(0.0);
                let fill_rect = egui::Rect::from_min_max(
                    rect.min,
                    egui::pos2(rect.left() + rect.width() * fill_frac, rect.max.y),
                );
                painter.rect_filled(fill_rect, 4.0, m.quality.color());
            });
        }
    }
}
