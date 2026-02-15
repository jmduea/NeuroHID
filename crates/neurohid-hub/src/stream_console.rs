//! # Stream Console
//!
//! A debug/monitoring console that displays live sample data from the DataBus.
//! Features filter/search, line numbers, auto-scroll control, and stats display.

use crate::data_bus::DataBus;
use crate::state::ServiceSnapshot;
use crate::theme;
use eframe::egui::{self, Color32, RichText, ScrollArea};
use std::collections::VecDeque;

/// Terminal-style dark background color.
const TERMINAL_BG: Color32 = Color32::from_gray(15);
/// Line number color.
const LINE_NUM_COLOR: Color32 = Color32::from_gray(70);
/// Highlight color for filter matches.
const HIGHLIGHT_COLOR: Color32 = Color32::from_rgb(255, 235, 59);

/// A scrolling console for viewing live sample streams.
pub struct StreamConsole {
    pub visible: bool,
    paused: bool,
    lines: VecDeque<String>,
    max_lines: usize,
    last_sample_count: usize,
    /// Filter text for searching lines.
    filter_text: String,
    /// Total lines received (for stats, even when filtered).
    total_lines_received: usize,
    /// Calculated data rate (lines per second).
    data_rate_lps: f32,
    /// Time of last rate calculation.
    last_rate_check_time: f64,
    /// Lines received since last rate check.
    lines_since_rate_check: usize,
    /// Whether user has manually scrolled up (disables auto-scroll).
    user_scrolled_up: bool,
    /// Number of new lines since user scrolled up.
    new_lines_while_scrolled: usize,
    /// Currently selected stream-type filter (None = show all).
    stream_type_filter: Option<String>,
}

impl Default for StreamConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamConsole {
    /// Create a new stream console (initially hidden).
    pub fn new() -> Self {
        Self {
            visible: false,
            paused: false,
            lines: VecDeque::new(),
            max_lines: 500,
            last_sample_count: 0,
            filter_text: String::new(),
            total_lines_received: 0,
            data_rate_lps: 0.0,
            last_rate_check_time: 0.0,
            lines_since_rate_check: 0,
            user_scrolled_up: false,
            new_lines_while_scrolled: 0,
            stream_type_filter: None,
        }
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Update with new samples from the data bus.
    /// Formats new samples and adds them to the console.
    pub fn update(&mut self, bus: &DataBus, snapshot: &ServiceSnapshot) {
        let total = bus.total_samples_received;

        // No new samples since last update
        if total <= self.last_sample_count as u64 {
            return;
        }

        let new_count = (total - self.last_sample_count as u64) as usize;

        // Only format new samples if not paused
        if !self.paused {
            // Take the last `new_count` samples from the ring buffer
            // (clamped to what's actually in the buffer)
            let available = bus.samples.len();
            let to_take = new_count.min(available);
            let skip = available.saturating_sub(to_take);

            for sample in bus.samples.iter().skip(skip) {
                let line = self.format_sample(sample, &snapshot.discovered_streams);
                self.lines.push_back(line);
                self.total_lines_received += 1;
                self.lines_since_rate_check += 1;

                // Trim to max lines
                if self.lines.len() > self.max_lines {
                    self.lines.pop_front();
                }
            }

            // Track new lines while user is scrolled up
            if self.user_scrolled_up {
                self.new_lines_while_scrolled += to_take;
            }
        }

        self.last_sample_count = total as usize;
    }

    /// Format a sample as a console line.
    fn format_sample(
        &self,
        sample: &neurohid_types::signal::Sample,
        discovered_streams: &[neurohid_types::device::DiscoveredStream],
    ) -> String {
        // Format timestamp: [HH:MM:SS.mmm]
        let timestamp_us = sample.system_timestamp;
        let timestamp_secs = timestamp_us / 1_000_000;
        let millis = (timestamp_us % 1_000_000) / 1_000;

        // Extract time-of-day components
        let time_of_day = timestamp_secs % 86400;
        let hours = (time_of_day / 3600) as u32;
        let minutes = ((time_of_day % 3600) / 60) as u32;
        let seconds = (time_of_day % 60) as u32;

        let time_str = format!("[{:02}:{:02}:{:02}.{:03}]", hours, minutes, seconds, millis);

        // Resolve stream type from source_id via discovered streams
        let source = sample.source_id.as_deref().unwrap_or("?");
        let stream_type = sample
            .source_id
            .as_ref()
            .and_then(|sid| {
                discovered_streams
                    .iter()
                    .find(|ds| ds.id == *sid)
                    .map(|ds| ds.stream_type.as_str())
            })
            .unwrap_or("?");

        // Format channel values (up to 8 channels)
        let mut values_str = String::new();
        let max_display = 8.min(sample.values.len());
        for (i, &val) in sample.values.iter().take(max_display).enumerate() {
            if i > 0 {
                values_str.push(' ');
            }
            values_str.push_str(&format!("ch{}={:.2}", i, val));
        }
        if sample.values.len() > max_display {
            values_str.push_str(" ...");
        }

        format!("{} [{}] [{}] {}", time_str, stream_type, source, values_str)
    }

    /// Update data rate calculation.
    fn update_rate(&mut self, current_time: f64) {
        let elapsed = current_time - self.last_rate_check_time;
        if elapsed >= 0.5 {
            self.data_rate_lps = self.lines_since_rate_check as f32 / elapsed as f32;
            self.lines_since_rate_check = 0;
            self.last_rate_check_time = current_time;
        }
    }

    /// Render the console UI.
    pub fn show(&mut self, ctx: &egui::Context, bus: &DataBus, snapshot: &ServiceSnapshot) {
        if !self.visible {
            return;
        }

        let current_time = ctx.input(|i| i.time);
        self.update_rate(current_time);

        egui::TopBottomPanel::bottom("stream_console")
            .resizable(true)
            .default_height(200.0)
            .min_height(100.0)
            .max_height(400.0)
            .frame(egui::Frame::NONE.fill(TERMINAL_BG))
            .show(ctx, |ui| {
                // Header bar
                self.show_header(ui);

                ui.add_space(2.0);

                // Filter input
                self.show_filter(ui, snapshot);

                ui.add_space(4.0);

                // Scrolling content area
                self.show_content(ui);

                // Stats line at bottom
                self.show_stats(ui, bus, snapshot);
            });
    }

    /// Render the header bar with title and controls.
    fn show_header(&mut self, ui: &mut egui::Ui) {
        let header_frame = egui::Frame::NONE
            .fill(Color32::from_gray(25))
            .inner_margin(egui::Margin::symmetric(8, 4));

        header_frame.show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("Stream Console")
                        .monospace()
                        .size(13.0)
                        .color(Color32::from_gray(200)),
                );

                theme::status_chip(
                    ui,
                    if self.paused { "Paused" } else { "Live" },
                    if self.paused {
                        theme::Intent::Warning
                    } else {
                        theme::Intent::Success
                    },
                );
                theme::status_chip(
                    ui,
                    &format!("Buffered {}", self.lines.len()),
                    if self.lines.is_empty() {
                        theme::Intent::Muted
                    } else {
                        theme::Intent::Info
                    },
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Close button
                    if theme::action_button(ui, "Close", true, theme::ButtonTone::Ghost) {
                        self.visible = false;
                    }

                    ui.add_space(8.0);

                    // Clear button
                    if theme::action_button(ui, "Clear", true, theme::ButtonTone::Secondary) {
                        self.lines.clear();
                        self.total_lines_received = 0;
                        self.new_lines_while_scrolled = 0;
                    }

                    ui.add_space(4.0);

                    // Pause/Resume button
                    let pause_label = if self.paused { "Resume" } else { "Pause" };
                    let pause_tone = if self.paused {
                        theme::ButtonTone::Primary
                    } else {
                        theme::ButtonTone::Ghost
                    };
                    if theme::action_button(ui, pause_label, true, pause_tone) {
                        self.paused = !self.paused;
                    }
                });
            });
        });
    }

    /// Render the filter input.
    fn show_filter(&mut self, ui: &mut egui::Ui, snapshot: &ServiceSnapshot) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);

            // Stream type filter buttons
            let streams = &snapshot.discovered_streams;
            if !streams.is_empty() {
                // Collect unique stream types
                let mut stream_types: Vec<&str> =
                    streams.iter().map(|ds| ds.stream_type.as_str()).collect();
                stream_types.sort_unstable();
                stream_types.dedup();

                // "All" toggle
                let all_selected = self.stream_type_filter.is_none();
                if theme::nav_button(ui, "All", all_selected).clicked() {
                    self.stream_type_filter = None;
                }

                // Per-type toggles
                for st in &stream_types {
                    let selected = self
                        .stream_type_filter
                        .as_deref()
                        .is_some_and(|f| f.eq_ignore_ascii_case(st));
                    if theme::nav_button(ui, st, selected).clicked() {
                        self.stream_type_filter = Some(st.to_string());
                    }
                }

                ui.separator();
            }

            ui.label(
                RichText::new("Filter:")
                    .monospace()
                    .size(11.0)
                    .color(Color32::from_gray(140)),
            );
            ui.add_space(4.0);

            let _ = theme::text_input(
                ui,
                "stream_console_filter_text",
                &mut self.filter_text,
                "type to filter...",
                200.0,
            );

            if !self.filter_text.is_empty() {
                if theme::action_button(ui, "Clear", true, theme::ButtonTone::Ghost) {
                    self.filter_text.clear();
                }

                // Show filtered count
                let filtered_count = self.get_filtered_lines().count();
                theme::status_chip(
                    ui,
                    &format!("{} matches", filtered_count),
                    theme::Intent::Info,
                );
            }
        });
    }

    /// Get an iterator over lines that match the filter.
    fn get_filtered_lines(&self) -> impl Iterator<Item = (usize, &String)> {
        let filter_lower = self.filter_text.to_lowercase();
        let stream_filter = self.stream_type_filter.clone();
        self.lines.iter().enumerate().filter(move |(_, line)| {
            // Text filter
            let text_match = filter_lower.is_empty() || line.to_lowercase().contains(&filter_lower);
            // Stream type filter (matches the [TYPE] tag in the formatted line)
            let stream_match = match &stream_filter {
                Some(st) => {
                    let tag = format!("[{}]", st);
                    line.contains(&tag)
                }
                None => true,
            };
            text_match && stream_match
        })
    }

    /// Render the scrolling content area.
    fn show_content(&mut self, ui: &mut egui::Ui) {
        let content_frame = egui::Frame::NONE
            .fill(TERMINAL_BG)
            .inner_margin(egui::Margin::symmetric(4, 4));

        content_frame.show(ui, |ui| {
            let available_height = ui.available_height() - 24.0; // Reserve space for stats

            // Determine if we should stick to bottom
            let stick_to_bottom = !self.user_scrolled_up && !self.paused;

            let scroll_area = ScrollArea::vertical()
                .max_height(available_height)
                .stick_to_bottom(stick_to_bottom)
                .auto_shrink([false, false]);

            let scroll_output = scroll_area.show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                let filter_lower = self.filter_text.to_lowercase();
                let stream_filter = &self.stream_type_filter;
                let line_num_width = format!("{:4}", self.lines.len()).len();

                for (idx, line) in self.lines.iter().enumerate() {
                    // Apply text filter
                    if !self.filter_text.is_empty() && !line.to_lowercase().contains(&filter_lower)
                    {
                        continue;
                    }
                    // Apply stream type filter
                    if let Some(st) = stream_filter {
                        let tag = format!("[{}]", st);
                        if !line.contains(&tag) {
                            continue;
                        }
                    }

                    ui.horizontal(|ui| {
                        // Line number (1-indexed for display)
                        let line_num = idx + 1;
                        ui.label(
                            RichText::new(format!("{:>width$} ", line_num, width = line_num_width))
                                .monospace()
                                .size(11.0)
                                .color(LINE_NUM_COLOR),
                        );

                        // Line content with optional highlighting
                        if !self.filter_text.is_empty() {
                            self.show_highlighted_line(ui, line, &filter_lower);
                        } else {
                            ui.label(
                                RichText::new(line)
                                    .monospace()
                                    .size(11.0)
                                    .color(Color32::from_gray(200)),
                            );
                        }
                    });
                }
            });

            // Detect if user scrolled up
            let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
            if scroll_delta > 0.0 {
                // Scrolling up
                self.user_scrolled_up = true;
            }

            // Check if we're at the bottom
            if scroll_output.state.offset.y
                >= scroll_output.content_size.y - available_height - 10.0
            {
                self.user_scrolled_up = false;
                self.new_lines_while_scrolled = 0;
            }

            // "New data below" badge when scrolled up
            if self.user_scrolled_up && self.new_lines_while_scrolled > 0 {
                let rect = ui.max_rect();
                let badge_pos = egui::pos2(rect.center().x - 60.0, rect.bottom() - 30.0);

                let badge_rect = egui::Rect::from_min_size(badge_pos, egui::vec2(120.0, 24.0));

                // Draw badge background
                ui.painter().rect_filled(
                    badge_rect,
                    4.0,
                    Color32::from_rgba_unmultiplied(33, 150, 243, 220),
                );

                // Badge text and click handling
                let badge_response = ui.allocate_rect(badge_rect, egui::Sense::click());
                if badge_response.clicked() {
                    self.user_scrolled_up = false;
                    self.new_lines_while_scrolled = 0;
                }

                ui.painter().text(
                    badge_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!(
                        "New data below ({}) \u{2193}",
                        self.new_lines_while_scrolled
                    ),
                    egui::FontId::monospace(10.0),
                    Color32::WHITE,
                );
            }
        });
    }

    /// Show a line with filter matches highlighted.
    fn show_highlighted_line(&self, ui: &mut egui::Ui, line: &str, filter_lower: &str) {
        let line_lower = line.to_lowercase();

        // Find all match positions
        let mut last_end = 0;
        let mut job = egui::text::LayoutJob::default();

        for (start, _) in line_lower.match_indices(filter_lower) {
            let end = start + self.filter_text.len();

            // Text before match
            if start > last_end {
                job.append(
                    &line[last_end..start],
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::monospace(11.0),
                        color: Color32::from_gray(200),
                        ..Default::default()
                    },
                );
            }

            // Highlighted match
            job.append(
                &line[start..end],
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(11.0),
                    color: Color32::BLACK,
                    background: HIGHLIGHT_COLOR,
                    ..Default::default()
                },
            );

            last_end = end;
        }

        // Remaining text after last match
        if last_end < line.len() {
            job.append(
                &line[last_end..],
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(11.0),
                    color: Color32::from_gray(200),
                    ..Default::default()
                },
            );
        }

        ui.label(job);
    }

    /// Render the stats line at the bottom.
    fn show_stats(&self, ui: &mut egui::Ui, bus: &DataBus, snapshot: &ServiceSnapshot) {
        let stats_frame = egui::Frame::NONE
            .fill(Color32::from_gray(20))
            .inner_margin(egui::Margin::symmetric(8, 4));

        stats_frame.show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let visible_lines =
                    if self.filter_text.is_empty() && self.stream_type_filter.is_none() {
                        self.lines.len()
                    } else {
                        self.get_filtered_lines().count()
                    };

                let buffer_pct = (self.lines.len() as f32 / self.max_lines as f32 * 100.0) as u32;

                // Build per-stream sample count summary
                let mut connected_streams = 0usize;
                let mut stream_info = String::new();
                for ds in &snapshot.discovered_streams {
                    if ds.connected {
                        connected_streams += 1;
                        let count = bus.samples_by_source.get(&ds.id).map_or(0, |b| b.len());
                        if !stream_info.is_empty() {
                            stream_info.push_str(", ");
                        }
                        stream_info.push_str(&format!("{}:{}", ds.stream_type, count));
                    }
                }

                theme::status_chip(
                    ui,
                    &format!("Lines {}/{}", visible_lines, self.max_lines),
                    if visible_lines > 0 {
                        theme::Intent::Info
                    } else {
                        theme::Intent::Muted
                    },
                );
                theme::status_chip(
                    ui,
                    &format!("Rate {:.0} sps", self.data_rate_lps),
                    if self.data_rate_lps > 0.0 {
                        theme::Intent::Success
                    } else {
                        theme::Intent::Muted
                    },
                );
                theme::status_chip(
                    ui,
                    &format!("Buffer {}%", buffer_pct),
                    if buffer_pct >= 80 {
                        theme::Intent::Success
                    } else if buffer_pct >= 30 {
                        theme::Intent::Info
                    } else {
                        theme::Intent::Muted
                    },
                );
                theme::status_chip(
                    ui,
                    &format!("Streams {}", connected_streams),
                    if connected_streams > 0 {
                        theme::Intent::Info
                    } else {
                        theme::Intent::Muted
                    },
                );

                if !stream_info.is_empty() {
                    theme::status_chip(
                        ui,
                        &format!("Streams {}", stream_info),
                        theme::Intent::Muted,
                    );
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    theme::status_chip(
                        ui,
                        if self.paused { "Paused" } else { "Live" },
                        if self.paused {
                            theme::Intent::Warning
                        } else {
                            theme::Intent::Success
                        },
                    );
                });
            });
        });
    }
}
