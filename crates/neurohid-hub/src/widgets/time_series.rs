//! # Time Series Widget
//!
//! Displays real-time scrolling EEG waveforms for all channels.
//! Each channel is rendered as a separate trace with a unique color.

use crate::widgets::{Widget, WidgetContext, WidgetId};
use eframe::egui;
use neurohid_types::event::{MarkerType, StreamMarker};

/// TODO: also in FFT plot — unify/dynamically generate based on stream metadata?
/// Channel colors matching common EEG GUI conventions.
const CHANNEL_COLORS: &[egui::Color32] = &[
    egui::Color32::from_rgb(129, 199, 132), // green
    egui::Color32::from_rgb(100, 181, 246), // blue
    egui::Color32::from_rgb(239, 154, 154), // red
    egui::Color32::from_rgb(255, 213, 79),  // yellow
    egui::Color32::from_rgb(206, 147, 216), // purple
    egui::Color32::from_rgb(255, 183, 77),  // orange
    egui::Color32::from_rgb(128, 222, 234), // cyan
    egui::Color32::from_rgb(240, 98, 146),  // pink
];

/// TODO: also in FFT plot — unify/dynamically generate based on stream metadata?
const CHANNEL_NAMES: &[&str] = &["AF3", "AF4", "T7", "T8", "Pz"];

/// TODO: Create a shared style module?
/// Left margin for amplitude scale indicator
const LEFT_MARGIN: f32 = 45.0;
/// Bottom margin for time axis
const BOTTOM_MARGIN: f32 = 24.0;

pub struct TimeSeriesWidget {
    /// Vertical scale in uV per division.
    vertical_scale: f32,
    /// Whether to auto-scale the vertical axis based on data.
    auto_scale: bool,
    /// Window duration in seconds.
    window_secs: f32,
    /// Which channels are enabled for display.
    channel_enabled: [bool; 8],
    /// Whether display is paused (data still collects but view is frozen).
    paused: bool,
    /// Cached samples when paused.
    paused_samples: Vec<neurohid_types::signal::Sample>,
    /// Smoothed per-channel DC offset (exponential moving average).
    /// Persists across frames so the baseline doesn't jump around.
    dc_offset: [f32; 8],
    /// Whether the DC offset has been initialised from data.
    dc_initialized: [bool; 8],
    /// Show marker overlays.
    show_markers: bool,
    /// Marker-type filters.
    show_marker_click: bool,
    show_marker_movement: bool,
    show_marker_head: bool,
    show_marker_errp: bool,
    /// Optional source-id substring filter.
    marker_source_filter: String,
    /// Optional bound source stream id.
    selected_source: Option<String>,
}

impl TimeSeriesWidget {
    pub fn new() -> Self {
        Self {
            vertical_scale: 200.0,
            auto_scale: true,
            window_secs: 5.0,
            channel_enabled: [true; 8],
            paused: false,
            paused_samples: Vec::new(),
            dc_offset: [0.0; 8],
            dc_initialized: [false; 8],
            show_markers: true,
            show_marker_click: true,
            show_marker_movement: true,
            show_marker_head: true,
            show_marker_errp: true,
            marker_source_filter: String::new(),
            selected_source: None,
        }
    }

    fn marker_passes_filters(&self, marker: &StreamMarker) -> bool {
        let type_ok = match marker.marker_type {
            MarkerType::MouseClick => self.show_marker_click,
            MarkerType::CursorMovement => self.show_marker_movement,
            MarkerType::HeadMovement => self.show_marker_head,
            MarkerType::ErrpWindowStart | MarkerType::ErrpWindowResult => self.show_marker_errp,
            _ => true,
        };
        if !type_ok {
            return false;
        }

        let filter = self.marker_source_filter.trim();
        if filter.is_empty() {
            return true;
        }
        marker
            .source_id
            .as_deref()
            .map(|s| s.contains(filter))
            .unwrap_or(false)
    }

    /// Draw the amplitude scale indicator on the left edge of a channel.
    fn draw_scale_indicator(
        painter: &egui::Painter,
        rect: egui::Rect,
        scale: f32,
        center_y: f32,
        channel_height: f32,
    ) {
        let scale_x = rect.left() - 3.0;
        let half_scale = scale / 2.0;
        let y_offset = channel_height * 0.4;

        // Vertical scale bar
        let top_y = center_y - y_offset;
        let bottom_y = center_y + y_offset;
        painter.line_segment(
            [egui::pos2(scale_x, top_y), egui::pos2(scale_x, bottom_y)],
            egui::Stroke::new(1.0, egui::Color32::from_gray(100)),
        );

        // Top tick and label
        painter.line_segment(
            [egui::pos2(scale_x - 3.0, top_y), egui::pos2(scale_x, top_y)],
            egui::Stroke::new(1.0, egui::Color32::from_gray(100)),
        );
        painter.text(
            egui::pos2(scale_x - 5.0, top_y),
            egui::Align2::RIGHT_CENTER,
            format!("+{:.0}", half_scale),
            egui::FontId::proportional(8.0),
            egui::Color32::from_gray(120),
        );

        // Center tick
        painter.line_segment(
            [
                egui::pos2(scale_x - 2.0, center_y),
                egui::pos2(scale_x, center_y),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
        );

        // Bottom tick and label
        painter.line_segment(
            [
                egui::pos2(scale_x - 3.0, bottom_y),
                egui::pos2(scale_x, bottom_y),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(100)),
        );
        painter.text(
            egui::pos2(scale_x - 5.0, bottom_y),
            egui::Align2::RIGHT_CENTER,
            format!("-{:.0}", half_scale),
            egui::FontId::proportional(8.0),
            egui::Color32::from_gray(120),
        );
    }

    /// Draw the time axis at the bottom of the plot area.
    fn draw_time_axis(painter: &egui::Painter, rect: egui::Rect, window_secs: f32) {
        let axis_y = rect.bottom() + 2.0;

        // Draw axis line
        painter.line_segment(
            [
                egui::pos2(rect.left(), axis_y),
                egui::pos2(rect.right(), axis_y),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
        );

        // Draw tick marks every 1 second
        let tick_interval = 1.0f32;
        let num_ticks = (window_secs / tick_interval) as i32 + 1;

        for i in 0..num_ticks {
            let time_offset = i as f32 * tick_interval;
            let x = rect.right() - (time_offset / window_secs) * rect.width();

            if x < rect.left() {
                break;
            }

            // Tick mark
            painter.line_segment(
                [egui::pos2(x, axis_y), egui::pos2(x, axis_y + 4.0)],
                egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
            );

            // Label (show negative time, right edge = 0)
            let label = if i == 0 {
                "0s".to_string()
            } else {
                format!("-{}s", i)
            };
            painter.text(
                egui::pos2(x, axis_y + 6.0),
                egui::Align2::CENTER_TOP,
                label,
                egui::FontId::proportional(9.0),
                egui::Color32::from_gray(120),
            );
        }
    }

    /// Draw horizontal grid lines for a channel.
    fn draw_channel_grid(
        painter: &egui::Painter,
        rect: egui::Rect,
        center_y: f32,
        channel_height: f32,
    ) {
        let grid_color = egui::Color32::from_gray(30);
        let y_offset = channel_height * 0.4;

        // Grid lines at +/- 50% and +/- 100% of scale
        for factor in &[0.5f32, 1.0] {
            let offset = y_offset * factor;
            // Above center
            painter.line_segment(
                [
                    egui::pos2(rect.left(), center_y - offset),
                    egui::pos2(rect.right(), center_y - offset),
                ],
                egui::Stroke::new(0.5, grid_color),
            );
            // Below center
            painter.line_segment(
                [
                    egui::pos2(rect.left(), center_y + offset),
                    egui::pos2(rect.right(), center_y + offset),
                ],
                egui::Stroke::new(0.5, grid_color),
            );
        }
    }

    /// Draw gradient fill between the waveform and the center line.
    fn draw_waveform_fill(
        painter: &egui::Painter,
        points: &[egui::Pos2],
        color: egui::Color32,
        center_y: f32,
    ) {
        if points.len() < 2 {
            return;
        }

        let fill_color = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 25); // ~10% opacity

        // Build triangle strips: each pair of adjacent waveform points forms
        // a quad with the center line, split into two triangles.
        // This avoids convex_polygon which produces artifacts on concave shapes.
        let mut mesh = egui::Mesh::default();
        for window in points.windows(2) {
            let p0 = window[0];
            let p1 = window[1];
            let b0 = egui::pos2(p0.x, center_y);
            let b1 = egui::pos2(p1.x, center_y);

            let base = mesh.vertices.len() as u32;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: p0,
                uv: egui::epaint::WHITE_UV,
                color: fill_color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: p1,
                uv: egui::epaint::WHITE_UV,
                color: fill_color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: b1,
                uv: egui::epaint::WHITE_UV,
                color: fill_color,
            });
            mesh.vertices.push(egui::epaint::Vertex {
                pos: b0,
                uv: egui::epaint::WHITE_UV,
                color: fill_color,
            });

            // Two triangles for the quad
            mesh.indices.extend_from_slice(&[base, base + 1, base + 2]);
            mesh.indices.extend_from_slice(&[base, base + 2, base + 3]);
        }

        painter.add(egui::Shape::mesh(mesh));
    }

    /// Draw crosshair and value tooltips.
    fn draw_crosshair(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        plot_rect: egui::Rect,
        hover_pos: egui::Pos2,
        visible_samples: &[&neurohid_types::signal::Sample],
        channel_rects: &[(usize, egui::Rect, f32)], // (channel_idx, rect, center_y)
        channel_means: &[f32; 8],
        effective_scale: f32,
        window_secs: f32,
    ) {
        // Only draw if within plot area horizontally
        if hover_pos.x < plot_rect.left() || hover_pos.x > plot_rect.right() {
            return;
        }

        // Vertical crosshair line (dashed effect via multiple segments)
        let dash_len = 4.0;
        let gap_len = 3.0;
        let mut y = plot_rect.top();
        while y < plot_rect.bottom() {
            let y_end = (y + dash_len).min(plot_rect.bottom());
            painter.line_segment(
                [egui::pos2(hover_pos.x, y), egui::pos2(hover_pos.x, y_end)],
                egui::Stroke::new(1.0, egui::Color32::from_gray(180)),
            );
            y += dash_len + gap_len;
        }

        // Calculate time at crosshair position
        let x_ratio = (hover_pos.x - plot_rect.left()) / plot_rect.width();
        let time_offset = (1.0 - x_ratio) * window_secs;
        let time_label = format!("t = -{:.2}s", time_offset);

        // Draw timestamp label at top of crosshair
        let label_pos = egui::pos2(hover_pos.x, plot_rect.top() - 2.0);
        let label_galley = ui.painter().layout_no_wrap(
            time_label.clone(),
            egui::FontId::proportional(10.0),
            egui::Color32::WHITE,
        );
        let label_rect = egui::Rect::from_min_size(
            egui::pos2(
                label_pos.x - label_galley.size().x / 2.0 - 4.0,
                label_pos.y - label_galley.size().y - 4.0,
            ),
            label_galley.size() + egui::vec2(8.0, 4.0),
        );
        painter.rect_filled(label_rect, 3.0, egui::Color32::from_gray(40));
        painter.text(
            egui::pos2(label_pos.x, label_pos.y - 2.0),
            egui::Align2::CENTER_BOTTOM,
            time_label,
            egui::FontId::proportional(10.0),
            egui::Color32::WHITE,
        );

        // Find sample index at crosshair
        let sample_idx = ((x_ratio) * visible_samples.len() as f32) as usize;
        let sample_idx = sample_idx.min(visible_samples.len().saturating_sub(1));

        if let Some(sample) = visible_samples.get(sample_idx) {
            // Draw value markers and tooltips for each channel
            for &(ch, ch_rect, center_y) in channel_rects {
                if let Some(value) = sample.get(ch) {
                    let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];
                    let channel_height = ch_rect.height();

                    // Calculate Y position of value (subtract DC offset)
                    let centered = value - channel_means[ch];
                    let y = center_y - (centered / effective_scale) * (channel_height * 0.4);
                    let y = y.clamp(ch_rect.top(), ch_rect.bottom());

                    // Draw marker circle at intersection
                    painter.circle_filled(egui::pos2(hover_pos.x, y), 3.0, color);
                    painter.circle_stroke(
                        egui::pos2(hover_pos.x, y),
                        3.0,
                        egui::Stroke::new(1.0, egui::Color32::WHITE),
                    );

                    // Draw value label
                    let value_label = format!("{:.1} uV", value);
                    let label_x = hover_pos.x + 8.0;
                    let label_y = y;

                    let galley = ui.painter().layout_no_wrap(
                        value_label.clone(),
                        egui::FontId::proportional(9.0),
                        color,
                    );
                    let bg_rect = egui::Rect::from_min_size(
                        egui::pos2(label_x - 2.0, label_y - galley.size().y / 2.0 - 1.0),
                        galley.size() + egui::vec2(4.0, 2.0),
                    );
                    painter.rect_filled(
                        bg_rect,
                        2.0,
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200),
                    );
                    painter.text(
                        egui::pos2(label_x, label_y),
                        egui::Align2::LEFT_CENTER,
                        value_label,
                        egui::FontId::proportional(9.0),
                        color,
                    );
                }
            }
        }
    }

    /// Draw empty state with pulsing indicator.
    fn draw_empty_state(ui: &mut egui::Ui) {
        ui.centered_and_justified(|ui| {
            ui.vertical_centered(|ui| {
                // Pulsing dot animation
                let time = ui.ctx().input(|i| i.time);
                let pulse = ((time * 2.0).sin() * 0.5 + 0.5) as f32;
                let alpha = (100.0 + pulse * 155.0) as u8;

                let dot_color = egui::Color32::from_rgba_unmultiplied(100, 181, 246, alpha);

                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter()
                    .circle_filled(rect.center(), 4.0 + pulse * 2.0, dot_color);

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Waiting for device connection...")
                        .weak()
                        .size(13.0),
                );
                ui.label(
                    egui::RichText::new("Connect a BCI device to see live EEG data")
                        .weak()
                        .size(11.0)
                        .color(egui::Color32::from_gray(100)),
                );

                // Request repaint for animation
                ui.ctx().request_repaint();
            });
        });
    }
}

impl Widget for TimeSeriesWidget {
    fn id(&self) -> WidgetId {
        WidgetId::TimeSeries
    }

    fn title(&self) -> &str {
        "Time Series"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>, pane_index: usize) {
        /// TODO: Get actual sample rate from stream metadata, and update when source changes.
        let sample_rate = 128.0f32;
        let source_options = ctx.candidate_sources_for(WidgetId::TimeSeries);
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

        // Settings bar
        ui.horizontal(|ui| {
            // Pause toggle
            let pause_text = if self.paused { "Resume" } else { "Pause" };
            let pause_color = if self.paused {
                egui::Color32::from_rgb(255, 193, 7)
            } else {
                ui.visuals().widgets.inactive.fg_stroke.color
            };
            if ui
                .button(egui::RichText::new(pause_text).color(pause_color))
                .clicked()
            {
                self.paused = !self.paused;
                if self.paused {
                    // Cache current samples when pausing
                    let window_samples = (self.window_secs * sample_rate) as usize;
                    let samples = ctx.samples_for_widget_source(
                        WidgetId::TimeSeries,
                        self.selected_source.as_deref(),
                    );
                    let start = if samples.len() > window_samples {
                        samples.len() - window_samples
                    } else {
                        0
                    };
                    self.paused_samples = samples.range(start..).cloned().collect();
                }
            }

            ui.separator();

            ui.label("Scale:");
            let mut auto = self.auto_scale;
            if ui
                .selectable_label(auto, "Auto")
                .on_hover_text("Automatically fit waveform to view")
                .clicked()
            {
                auto = !auto;
            }
            self.auto_scale = auto;
            if !self.auto_scale {
                let drag = egui::DragValue::new(&mut self.vertical_scale)
                    .speed(5.0)
                    .clamp_range(1.0..=50000.0)
                    .suffix(" µV")
                    .min_decimals(0)
                    .max_decimals(0);
                ui.add(drag);
            }

            ui.label("Window:");
            egui::ComboBox::from_id_source(format!("ts_window_{}", pane_index))
                .selected_text(format!("{:.0}s", self.window_secs))
                .width(60.0)
                .show_ui(ui, |ui: &mut egui::Ui| {
                    for &v in &[1.0, 2.0, 5.0, 10.0, 20.0, 30.0f32] {
                        ui.selectable_value(&mut self.window_secs, v, format!("{:.0}s", v));
                    }
                });

            if !source_options.is_empty() {
                ui.separator();
                ui.label("Source:");
                egui::ComboBox::from_id_source(format!("ts_src_{}", pane_index))
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

            // Channel toggles
            ui.separator();
            // Use the discovered stream's channel count for stability —
            // avoids flickering when the flat buffer mixes stream types.
            let num_ch = ctx
                .channel_count_for_source(self.selected_source.as_deref().unwrap_or_default())
                .or_else(|| ctx.channel_count_for(&["EEG"]))
                .unwrap_or_else(|| {
                    ctx.samples_for_widget_source(
                        WidgetId::TimeSeries,
                        self.selected_source.as_deref(),
                    )
                    .back()
                    .map(|s| s.channel_count())
                    .unwrap_or(5)
                })
                .min(8);
            for ch in 0..num_ch {
                let name = CHANNEL_NAMES.get(ch).unwrap_or(&"?");
                let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];
                let mut enabled = self.channel_enabled[ch];
                if ui
                    .checkbox(
                        &mut enabled,
                        egui::RichText::new(*name).color(color).small(),
                    )
                    .changed()
                {
                    self.channel_enabled[ch] = enabled;
                }
            }

            ui.separator();
            ui.checkbox(&mut self.show_markers, "Markers");
            if self.show_markers {
                ui.checkbox(&mut self.show_marker_click, "Click");
                ui.checkbox(&mut self.show_marker_movement, "Move");
                ui.checkbox(&mut self.show_marker_head, "Head");
                ui.checkbox(&mut self.show_marker_errp, "ErrP");
                ui.label("Src:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.marker_source_filter)
                        .desired_width(70.0)
                        .hint_text("id"),
                );
            }

            ui.separator();

            // Sample rate and count display
            let total_samples = ctx
                .samples_for_widget_source(WidgetId::TimeSeries, self.selected_source.as_deref())
                .len();
            ui.label(
                egui::RichText::new(format!("{:.0} Hz | {} samples", sample_rate, total_samples))
                    .weak()
                    .small(),
            );
        });

        // Determine how many samples correspond to the window
        let window_samples = (self.window_secs * sample_rate) as usize;

        // Get the relevant samples (either from cache if paused or live)
        let visible_samples: Vec<&neurohid_types::signal::Sample> = if self.paused {
            self.paused_samples.iter().collect()
        } else {
            let samples = ctx
                .samples_for_widget_source(WidgetId::TimeSeries, self.selected_source.as_deref());
            let start = if samples.len() > window_samples {
                samples.len() - window_samples
            } else {
                0
            };
            samples.range(start..).collect()
        };

        if visible_samples.is_empty() {
            Self::draw_empty_state(ui);
            return;
        }

        // Use discovered stream channel count for stability;
        // fall back to sample-derived count only if no stream metadata.
        let num_channels = ctx
            .channel_count_for_source(self.selected_source.as_deref().unwrap_or_default())
            .or_else(|| ctx.channel_count_for(&["EEG"]))
            .unwrap_or_else(|| visible_samples[0].channel_count())
            .min(8);

        // Update smoothed per-channel DC offset using an EMA.
        // On the first frame we seed directly from the window mean;
        // after that we blend slowly so the baseline stays stable.
        // Alpha ≈ 0.005 → time-constant of ~200 frames (~1.5 s at 128 Hz repaint).
        const DC_ALPHA: f32 = 0.005;
        if !self.paused {
            for ch in 0..num_channels {
                if !self.channel_enabled[ch] {
                    continue;
                }
                let mut sum = 0.0f64;
                let mut count = 0u32;
                for sample in &visible_samples {
                    if let Some(v) = sample.get(ch) {
                        sum += v as f64;
                        count += 1;
                    }
                }
                if count > 0 {
                    let window_mean = (sum / count as f64) as f32;
                    if !self.dc_initialized[ch] {
                        // First data — seed directly so we don't start at 0.
                        self.dc_offset[ch] = window_mean;
                        self.dc_initialized[ch] = true;
                    } else {
                        // Smooth update: blend towards window mean.
                        self.dc_offset[ch] += DC_ALPHA * (window_mean - self.dc_offset[ch]);
                    }
                }
            }
        }
        let channel_means = self.dc_offset;

        // Compute effective vertical scale (auto or manual)
        let effective_scale = if self.auto_scale {
            let mut max_dev: f32 = 0.0;
            for ch in 0..num_channels {
                if !self.channel_enabled[ch] {
                    continue;
                }
                for sample in &visible_samples {
                    if let Some(v) = sample.get(ch) {
                        let dev = (v - channel_means[ch]).abs();
                        if dev > max_dev {
                            max_dev = dev;
                        }
                    }
                }
            }
            // Round up to a nice value with 20% headroom
            let target = (max_dev * 1.2).max(1.0);
            let nice_values = [
                10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0f32,
            ];
            nice_values
                .iter()
                .find(|&&v| v >= target)
                .copied()
                .unwrap_or(target.ceil())
        } else {
            self.vertical_scale
        };

        // Count enabled channels for layout
        let enabled_count = (0..num_channels)
            .filter(|&ch| self.channel_enabled[ch])
            .count();

        if enabled_count == 0 {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("No channels enabled").weak());
            });
            return;
        }

        let available = ui.available_size();
        let plot_width = available.x - LEFT_MARGIN;
        let plot_height = available.y - BOTTOM_MARGIN;
        let channel_height = plot_height / enabled_count as f32;

        // Allocate space for the entire plot area including margins
        let (total_rect, response) = ui.allocate_exact_size(available, egui::Sense::hover());

        let plot_rect = egui::Rect::from_min_size(
            egui::pos2(total_rect.left() + LEFT_MARGIN, total_rect.top()),
            egui::vec2(plot_width, plot_height),
        );

        // Track channel rects for crosshair
        let mut channel_rects: Vec<(usize, egui::Rect, f32)> = Vec::new();

        // Draw each enabled channel
        let mut y_offset = 0.0;
        for ch in 0..num_channels {
            if !self.channel_enabled[ch] {
                continue;
            }

            let color = CHANNEL_COLORS[ch % CHANNEL_COLORS.len()];
            let rect = egui::Rect::from_min_size(
                egui::pos2(plot_rect.left(), plot_rect.top() + y_offset),
                egui::vec2(plot_width, channel_height),
            );

            let center_y = rect.center().y;
            channel_rects.push((ch, rect, center_y));

            if ui.is_rect_visible(rect) {
                let painter = ui.painter_at(total_rect);

                // Background
                painter.rect_filled(rect, 0.0, egui::Color32::from_gray(20));

                // Separator line at bottom
                painter.line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    egui::Stroke::new(0.5, egui::Color32::from_gray(50)),
                );

                // Draw grid lines
                Self::draw_channel_grid(&painter, rect, center_y, channel_height);

                // Zero line (more prominent)
                painter.line_segment(
                    [
                        egui::pos2(rect.left(), center_y),
                        egui::pos2(rect.right(), center_y),
                    ],
                    egui::Stroke::new(0.5, egui::Color32::from_gray(50)),
                );

                // Channel label
                let name = CHANNEL_NAMES.get(ch).unwrap_or(&"?");
                painter.text(
                    rect.left_top() + egui::vec2(4.0, 2.0),
                    egui::Align2::LEFT_TOP,
                    *name,
                    egui::FontId::proportional(10.0),
                    color,
                );

                // Draw amplitude scale indicator
                Self::draw_scale_indicator(
                    &painter,
                    rect,
                    effective_scale,
                    center_y,
                    channel_height,
                );

                // Plot the waveform
                let n = visible_samples.len();
                if n > 1 {
                    let points: Vec<egui::Pos2> = visible_samples
                        .iter()
                        .enumerate()
                        .filter_map(|(i, sample)| {
                            let value = sample.get(ch)?;
                            let centered = value - channel_means[ch];
                            let x = rect.left() + (i as f32 / n as f32) * rect.width();
                            let y =
                                center_y - (centered / effective_scale) * (channel_height * 0.4);
                            let y = y.clamp(rect.top(), rect.bottom());
                            Some(egui::pos2(x, y))
                        })
                        .collect();

                    if points.len() >= 2 {
                        // Downsample for rendering performance if too many points
                        let max_points = (rect.width() as usize * 2).max(1);
                        let render_points = if points.len() > max_points {
                            let step = points.len() / max_points;
                            points.iter().step_by(step).copied().collect::<Vec<_>>()
                        } else {
                            points.clone()
                        };

                        // Draw gradient fill under waveform
                        Self::draw_waveform_fill(&painter, &render_points, color, center_y);

                        // Draw waveform line (wider stroke for better visibility)
                        painter.add(egui::Shape::line(
                            render_points,
                            egui::Stroke::new(1.5, color),
                        ));
                    }
                }
            }

            y_offset += channel_height;
        }

        // Draw time axis at bottom
        let painter = ui.painter_at(total_rect);
        Self::draw_time_axis(&painter, plot_rect, self.window_secs);

        // Draw marker overlays aligned to the current time window.
        if self.show_markers {
            let now_us = visible_samples
                .last()
                .map(|s| s.system_timestamp)
                .unwrap_or_default();
            let window_us = (self.window_secs * 1_000_000.0).max(1.0) as i64;
            let min_us = now_us.saturating_sub(window_us);

            for marker in ctx.bus.markers.iter() {
                if marker.timestamp < min_us || marker.timestamp > now_us {
                    continue;
                }
                if !self.marker_passes_filters(marker) {
                    continue;
                }

                let t = (marker.timestamp - min_us) as f32 / window_us as f32;
                let x = plot_rect.left() + t.clamp(0.0, 1.0) * plot_rect.width();
                let color = match marker.marker_type {
                    MarkerType::MouseClick => egui::Color32::from_rgb(129, 199, 132),
                    MarkerType::CursorMovement => egui::Color32::from_rgb(100, 181, 246),
                    MarkerType::HeadMovement => egui::Color32::from_rgb(255, 193, 7),
                    MarkerType::ErrpWindowStart => egui::Color32::from_rgb(244, 67, 54),
                    MarkerType::ErrpWindowResult => egui::Color32::from_rgb(206, 147, 216),
                    _ => egui::Color32::LIGHT_GRAY,
                };

                painter.line_segment(
                    [
                        egui::pos2(x, plot_rect.top()),
                        egui::pos2(x, plot_rect.bottom()),
                    ],
                    egui::Stroke::new(1.0, color.gamma_multiply(0.7)),
                );
                painter.circle_filled(egui::pos2(x, plot_rect.top() + 6.0), 2.5, color);
            }
        }

        // Draw crosshair if hovering
        if let Some(hover_pos) = response.hover_pos() {
            if plot_rect.contains(hover_pos) {
                self.draw_crosshair(
                    ui,
                    &painter,
                    plot_rect,
                    hover_pos,
                    &visible_samples,
                    &channel_rects,
                    &channel_means,
                    effective_scale,
                    self.window_secs,
                );
            }
        }
    }
}
