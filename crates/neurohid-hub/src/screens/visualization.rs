//! # Visualization Screen
//!
//! The main real-time data visualization screen. Uses the LayoutManager to
//! display multiple widgets (Time Series, FFT, Band Power, etc.) in a
//! configurable multi-pane layout.

use eframe::egui::{self, Color32, RichText};
use neurohid_types::config::UiConfig;

use crate::data_bus::DataBus;
use crate::layout::{LayoutConfig, LayoutManager};
use crate::state::{HubState, ServiceSnapshot};
use crate::widgets::WidgetContext;

/// Data freshness threshold in seconds.
const STALE_DATA_THRESHOLD_SECS: f64 = 2.0;

/// The visualization screen manages the layout and renders all active widgets.
pub struct VisualizationScreen {
    layout: LayoutManager,
    /// Track total samples received for rate calculation (monotonic counter).
    last_total_samples: u64,
    /// Time when last_total_samples was recorded.
    last_rate_check_time: f64,
    /// Calculated data rate (samples per second).
    data_rate_sps: f32,
    /// Time when streaming started (first sample received).
    stream_start_time: Option<f64>,
    /// Time when last sample was received.
    last_sample_time: Option<f64>,
    /// Animation phase for pulsing indicators.
    pulse_phase: f64,
}

impl Default for VisualizationScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl VisualizationScreen {
    pub fn new() -> Self {
        Self {
            layout: LayoutManager::new(),
            last_total_samples: 0,
            last_rate_check_time: 0.0,
            data_rate_sps: 0.0,
            stream_start_time: None,
            last_sample_time: None,
            pulse_phase: 0.0,
        }
    }

    pub fn from_ui_config(ui_config: &UiConfig) -> Self {
        Self {
            layout: LayoutManager::from_persisted(
                &ui_config.visualization_layout_preset,
                &ui_config.visualization_pane_widgets,
                ui_config.visualization_layout_tree_json.as_deref(),
            ),
            last_total_samples: 0,
            last_rate_check_time: 0.0,
            data_rate_sps: 0.0,
            stream_start_time: None,
            last_sample_time: None,
            pulse_phase: 0.0,
        }
    }

    /// Render the visualization screen.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        bus: &DataBus,
        snapshot: &ServiceSnapshot,
        state: &mut HubState,
        runtime: &tokio::runtime::Runtime,
    ) {
        ui.label(
            RichText::new("Visualization")
                .text_style(egui::TextStyle::Heading)
                .color(Color32::from_rgb(225, 233, 245)),
        );
        ui.label(
            RichText::new("Live neural telemetry with configurable analysis workspaces")
                .small()
                .color(Color32::from_rgb(128, 145, 167)),
        );
        ui.add_space(6.0);

        let current_time = ui.input(|i| i.time);
        self.update_metrics(bus, current_time);

        let ctx = WidgetContext { bus, snapshot };

        // Enhanced toolbar with professional styling
        self.show_toolbar(ui, bus, snapshot, current_time);

        // Check if we should show the welcome/empty state
        if !snapshot.running {
            self.show_welcome_panel(ui);
        } else {
            // Reserve space for footer, then render panes in remaining area
            let footer_height = 28.0;
            let available = ui.available_size();
            let pane_height = (available.y - footer_height).max(0.0);
            ui.allocate_ui(egui::vec2(available.x, pane_height), |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(6, 4))
                    .show(ui, |ui| {
                        self.layout.show_panes(ui, &ctx);
                    });
            });

            // Footer status strip
            self.show_footer(ui);
        }

        self.persist_layout_state(state, runtime);
    }

    fn persist_layout_state(&mut self, state: &mut HubState, runtime: &tokio::runtime::Runtime) {
        let Some(persisted) = self.layout.take_persisted_state() else {
            return;
        };

        state.config.ui.visualization_layout_preset = persisted.layout_preset;
        state.config.ui.visualization_pane_widgets = persisted.pane_widgets;
        state.config.ui.visualization_layout_tree_json = persisted.tree_json;

        if let Err(error) = runtime.block_on(state.config_store.save(&state.config)) {
            tracing::warn!("Failed to persist visualization layout state: {}", error);
        }
    }

    /// Update metrics for data rate and freshness tracking.
    fn update_metrics(&mut self, bus: &DataBus, current_time: f64) {
        let total = bus.total_samples_received;

        // Update pulse animation
        self.pulse_phase = (current_time * 2.0) % (2.0 * std::f64::consts::PI);

        // Track when data first arrives
        if total > 0 && self.stream_start_time.is_none() {
            self.stream_start_time = Some(current_time);
        }

        // Update last sample time when we have new data
        if total > self.last_total_samples {
            self.last_sample_time = Some(current_time);
        }

        // Calculate data rate every 0.5 seconds
        let elapsed = current_time - self.last_rate_check_time;
        if elapsed >= 0.5 {
            let sample_delta = total.saturating_sub(self.last_total_samples);
            self.data_rate_sps = sample_delta as f32 / elapsed as f32;
            self.last_total_samples = total;
            self.last_rate_check_time = current_time;
        }

        // Reset tracking when bus is disconnected
        if total == 0 {
            self.stream_start_time = None;
            self.last_sample_time = None;
            self.data_rate_sps = 0.0;
        }
    }

    /// Render the enhanced toolbar.
    fn show_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        bus: &DataBus,
        snapshot: &ServiceSnapshot,
        current_time: f64,
    ) {
        // Toolbar frame with subtle background and bottom border
        let toolbar_frame = egui::Frame::NONE
            .fill(ui.style().visuals.window_fill.gamma_multiply(0.95))
            .inner_margin(egui::Margin::symmetric(8, 6));

        toolbar_frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // Layout selector with icons
                self.show_layout_selector(ui);

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                // Data rate indicator
                self.show_data_rate(ui);

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                // Buffer health indicator
                self.show_buffer_health(ui, bus, snapshot);

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                // Recording placeholder (future-ready)
                ui.colored_label(Color32::from_gray(80), "REC");

                // Right-aligned status cluster
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Elapsed time
                    if let Some(start) = self.stream_start_time {
                        let elapsed = current_time - start;
                        ui.label(format_elapsed(elapsed));
                        ui.add_space(12.0);
                    }

                    // Connection status
                    self.show_connection_status(ui, bus, snapshot, current_time);

                    // Data freshness warning
                    if let Some(last) = self.last_sample_time {
                        let stale_secs = current_time - last;
                        if stale_secs > STALE_DATA_THRESHOLD_SECS && snapshot.running {
                            ui.add_space(12.0);
                            ui.colored_label(
                                Color32::from_rgb(255, 152, 0),
                                format!("Data stale - last {:.1}s ago", stale_secs),
                            );
                        }
                    }
                });
            });
        });

        // Bottom border line
        let rect = ui.available_rect_before_wrap();
        ui.painter().hline(
            rect.x_range(),
            rect.top(),
            egui::Stroke::new(
                1.0,
                ui.style().visuals.widgets.noninteractive.bg_stroke.color,
            ),
        );

        ui.add_space(4.0);
    }

    /// Layout selector with Unicode icons.
    fn show_layout_selector(&mut self, ui: &mut egui::Ui) {
        ui.label("Layout:");

        let current_label = format!(
            "{} {}",
            layout_icon(self.layout.config),
            self.layout.config.label()
        );
        let mut selected_layout = self.layout.config;

        egui::ComboBox::from_id_salt("layout_selector")
            .selected_text(current_label)
            .show_ui(ui, |ui| {
                for &layout in LayoutConfig::ALL {
                    let label = format!("{} {}", layout_icon(layout), layout.label());
                    if ui
                        .selectable_value(&mut selected_layout, layout, label)
                        .changed()
                    {
                        self.layout.set_layout(selected_layout);
                    }
                }
            });
    }

    /// Data rate display.
    fn show_data_rate(&self, ui: &mut egui::Ui) {
        let rate_text = if self.data_rate_sps > 0.0 {
            format!("{:.0} sps", self.data_rate_sps)
        } else {
            "-- sps".to_string()
        };

        ui.label(RichText::new(rate_text).monospace());
    }

    /// Buffer health indicator.
    fn show_buffer_health(&self, ui: &mut egui::Ui, bus: &DataBus, snapshot: &ServiceSnapshot) {
        const MAX_SAMPLES: usize = 1280;

        let streams = &snapshot.discovered_streams;
        let has_streams = streams.iter().any(|ds| ds.connected);

        if has_streams {
            // Show per-stream buffer counts
            for ds in streams {
                if !ds.connected {
                    continue;
                }
                let count = bus.samples_by_source.get(&ds.id).map_or(0, |b| b.len());
                let ratio = count as f32 / MAX_SAMPLES as f32;

                let color = if ratio > 0.9 {
                    Color32::from_rgb(76, 175, 80)
                } else if ratio > 0.5 {
                    Color32::from_rgb(255, 193, 7)
                } else if ratio > 0.0 {
                    Color32::from_rgb(33, 150, 243)
                } else {
                    Color32::from_gray(100)
                };

                ui.colored_label(color, format!("{}:{}", ds.stream_type, count));
            }
        } else {
            // Fallback: flat buffer view
            let count = bus.samples.len();
            let ratio = count as f32 / MAX_SAMPLES as f32;

            let color = if ratio > 0.9 {
                Color32::from_rgb(76, 175, 80)
            } else if ratio > 0.5 {
                Color32::from_rgb(255, 193, 7)
            } else if ratio > 0.0 {
                Color32::from_rgb(33, 150, 243)
            } else {
                Color32::from_gray(100)
            };

            ui.colored_label(color, format!("{}/{}", count, MAX_SAMPLES));
        }

        // Mini progress bar based on total samples across all streams
        let total_count: usize = if has_streams {
            bus.samples_by_source
                .values()
                .map(|b| b.len())
                .max()
                .unwrap_or(0)
        } else {
            bus.samples.len()
        };
        let ratio = total_count as f32 / MAX_SAMPLES as f32;

        let (rect, _) = ui.allocate_exact_size(egui::vec2(40.0, 8.0), egui::Sense::hover());
        let painter = ui.painter();

        let bar_color = if ratio > 0.9 {
            Color32::from_rgb(76, 175, 80)
        } else if ratio > 0.5 {
            Color32::from_rgb(255, 193, 7)
        } else if ratio > 0.0 {
            Color32::from_rgb(33, 150, 243)
        } else {
            Color32::from_gray(100)
        };

        painter.rect_filled(rect, 2.0, Color32::from_gray(40));
        let fill_width = rect.width() * ratio.min(1.0);
        let fill_rect = egui::Rect::from_min_size(rect.min, egui::vec2(fill_width, rect.height()));
        painter.rect_filled(fill_rect, 2.0, bar_color);
    }

    /// Connection status indicator.
    fn show_connection_status(
        &mut self,
        ui: &mut egui::Ui,
        bus: &DataBus,
        snapshot: &ServiceSnapshot,
        current_time: f64,
    ) {
        if !snapshot.running {
            ui.colored_label(Color32::from_rgb(244, 67, 54), "\u{25CF} Offline");
        } else if bus.samples.is_empty() {
            let pulse = (self.pulse_phase.sin() * 0.5 + 0.5) as f32;
            let alpha = (128.0 + pulse * 127.0) as u8;
            let color = Color32::from_rgba_unmultiplied(255, 193, 7, alpha);
            ui.colored_label(color, "\u{25CF} Connecting...");
            ui.ctx().request_repaint();
        } else {
            let pulse = ((current_time * 2.4).sin() * 0.5 + 0.5) as f32;
            let live_color = Color32::from_rgba_unmultiplied(
                106,
                227,
                130,
                (180.0 + pulse * 75.0) as u8,
            );

            // Show stream count and active stream types
            let connected: Vec<&str> = snapshot
                .discovered_streams
                .iter()
                .filter(|ds| ds.connected)
                .map(|ds| ds.stream_type.as_str())
                .collect();

            let label = if connected.is_empty() {
                format!("\u{25CF} Live - {} samples", bus.samples.len())
            } else {
                let mut types = connected;
                types.sort_unstable();
                types.dedup();
                format!(
                    "\u{25CF} Live - {} stream{} ({})",
                    types.len(),
                    if types.len() == 1 { "" } else { "s" },
                    types.join(", ")
                )
            };

            ui.colored_label(live_color, label);
            ui.ctx().request_repaint();
        }
    }

    /// Welcome/empty state panel when service is not running.
    fn show_welcome_panel(&self, ui: &mut egui::Ui) {
        let available = ui.available_size();

        ui.allocate_ui(available, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(available.y * 0.3);

                egui::Frame::group(ui.style())
                    .fill(Color32::from_rgb(20, 25, 34))
                    .show(ui, |ui| {
                        ui.heading(RichText::new("NeuroHID Visualization").size(24.0));
                        ui.add_space(12.0);

                        ui.label(
                            RichText::new("Start the service to begin streaming data.")
                                .size(14.0)
                                .color(Color32::from_gray(150)),
                        );

                        ui.add_space(8.0);

                        ui.label(
                            RichText::new("Go to the Dashboard screen to start the service.")
                                .size(12.0)
                                .color(Color32::from_gray(120)),
                        );
                    });
            });
        });
    }

    /// Footer status strip with contextual help.
    fn show_footer(&self, ui: &mut egui::Ui) {
        ui.add_space(4.0);

        let footer_frame = egui::Frame::NONE
            .fill(ui.style().visuals.window_fill.gamma_multiply(0.92))
            .inner_margin(egui::Margin::symmetric(8, 4));

        footer_frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                let layout_name = self.layout.config.label();
                let pane_count = self.layout.config.pane_count();

                ui.label(
                    RichText::new(format!(
                        "Layout: {} | {} widgets active | Select widgets from dropdown menus",
                        layout_name, pane_count
                    ))
                    .size(11.0)
                    .color(Color32::from_gray(140)),
                );
            });
        });
    }
}

/// Get the Unicode icon for a layout configuration.
fn layout_icon(config: LayoutConfig) -> &'static str {
    match config {
        LayoutConfig::Single => "\u{25A3}",          // ▣
        LayoutConfig::TwoColumns => "\u{25EB}",      // ◫
        LayoutConfig::TwoRows => "\u{2B12}",         // ⬒ (or similar)
        LayoutConfig::Grid2x2 => "\u{229E}",         // ⊞
        LayoutConfig::OneLeftTwoRight => "\u{25E7}", // ◧
        LayoutConfig::TwoLeftOneRight => "\u{25E8}", // ◨
    }
}

/// Format elapsed time as HH:MM:SS or MM:SS.
fn format_elapsed(secs: f64) -> String {
    let total_secs = secs as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}
