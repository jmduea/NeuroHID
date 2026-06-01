//! # Stream Metadata Widget
//!
//! Always-on metadata surface for discovered and connected streams.

use crate::theme;
use crate::widgets::{Widget, WidgetContext, WidgetId};
use eframe::egui;

pub struct StreamMetadataWidget;

impl Default for StreamMetadataWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamMetadataWidget {
    pub fn new() -> Self {
        Self
    }
}

impl Widget for StreamMetadataWidget {
    fn id(&self) -> WidgetId {
        WidgetId::StreamMetadata
    }

    fn title(&self) -> &str {
        "Stream Metadata"
    }

    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>, pane_index: usize) {
        let streams = &ctx.snapshot.discovered_streams;
        if streams.is_empty() {
            theme::status_chip(ui, "No streams discovered", theme::Intent::Warning);
            return;
        }

        let connected_count = streams.iter().filter(|stream| stream.connected).count();
        ui.horizontal_wrapped(|ui| {
            theme::status_chip(
                ui,
                &format!("Discovered {}", streams.len()),
                theme::Intent::Info,
            );
            theme::status_chip(
                ui,
                &format!("Connected {}", connected_count),
                if connected_count > 0 {
                    theme::Intent::Success
                } else {
                    theme::Intent::Muted
                },
            );
        });
        ui.add_space(6.0);

        egui::Grid::new(format!("stream_meta_grid_{pane_index}"))
            .striped(true)
            .min_col_width(64.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Name").strong());
                ui.label(egui::RichText::new("Type").strong());
                ui.label(egui::RichText::new("Route").strong());
                ui.label(egui::RichText::new("Rate").strong());
                ui.label(egui::RichText::new("Drop").strong());
                ui.label(egui::RichText::new("Age").strong());
                ui.label(egui::RichText::new("Ch").strong());
                ui.label(egui::RichText::new("Battery").strong());
                ui.label(egui::RichText::new("Integrity").strong());
                ui.label(egui::RichText::new("Connected").strong());
                ui.end_row();

                for s in streams {
                    ui.label(&s.name);
                    ui.label(&s.stream_type);
                    let route = stream_route_hint(s);
                    theme::status_chip(ui, route, route_intent(route));
                    ui.label(match s.effective_sample_rate_hz {
                        Some(effective_hz) => format!("{effective_hz:.1}/{:.0} Hz", s.sample_rate),
                        None => format!("{:.0} Hz", s.sample_rate),
                    });
                    theme::status_chip(
                        ui,
                        &s.drop_rate_pct
                            .map(|value| format!("{value:.1}%"))
                            .unwrap_or_else(|| "n/a".to_string()),
                        drop_rate_intent(s.drop_rate_pct),
                    );
                    theme::status_chip(
                        ui,
                        &s.last_sample_age_ms
                            .map(|value| format!("{value}ms"))
                            .unwrap_or_else(|| "n/a".to_string()),
                        staleness_intent(s.last_sample_age_ms),
                    );
                    ui.label(format!("{}", s.channel_count));
                    if let Some(level) = s.battery_percent {
                        let battery_intent = if level >= 50 {
                            theme::Intent::Success
                        } else if level >= 20 {
                            theme::Intent::Warning
                        } else {
                            theme::Intent::Danger
                        };
                        theme::status_chip(ui, &format!("{}%", level), battery_intent);
                    } else {
                        theme::status_chip(ui, "n/a", theme::Intent::Muted);
                    }
                    let integrity = s.integrity_state.as_deref().unwrap_or("unknown");
                    let integrity_intent = match integrity {
                        "ok" => theme::Intent::Success,
                        "degraded" => theme::Intent::Warning,
                        _ => theme::Intent::Muted,
                    };
                    theme::status_chip(ui, integrity, integrity_intent);
                    if s.connected {
                        theme::status_chip(ui, "yes", theme::Intent::Success);
                    } else {
                        theme::status_chip(ui, "no", theme::Intent::Muted);
                    }
                    ui.end_row();

                    ui.label("");
                    ui.label(
                        egui::RichText::new(
                            s.preprocessing_summary
                                .as_deref()
                                .unwrap_or("preprocessing pending"),
                        )
                        .small()
                        .weak(),
                    );
                    ui.label("");
                    ui.label("");
                    ui.label("");
                    ui.label("");
                    ui.label("");
                    ui.label("");
                    ui.label("");
                    ui.end_row();
                }
            });

        ui.add_space(8.0);
        ui.label(egui::RichText::new("Source IDs").small().strong());
        for s in streams {
            ui.horizontal_wrapped(|ui| {
                theme::status_chip(ui, &s.name, theme::Intent::Info);
                theme::status_chip(
                    ui,
                    &format!(
                        "source {}",
                        s.source_id.clone().unwrap_or_else(|| "<none>".to_string())
                    ),
                    theme::Intent::Muted,
                );
            });
        }
    }
}

fn stream_route_hint(stream: &neurohid_types::device::DiscoveredStream) -> &'static str {
    let combined = format!(
        "{} {} {}",
        stream.stream_type.to_ascii_lowercase(),
        stream.name.to_ascii_lowercase(),
        stream.id.to_ascii_lowercase()
    );

    if ["motion", "acc", "imu", "gyro"]
        .iter()
        .any(|token| combined.contains(token))
    {
        return "motion";
    }
    if [
        "quality",
        "metric",
        "bandpower",
        "mental",
        "facial",
        "marker",
        "command",
        "devicequality",
    ]
    .iter()
    .any(|token| combined.contains(token))
    {
        return "auxiliary";
    }
    if combined.contains("eeg") && stream.channel_count >= 2 {
        return "eeg";
    }
    "unknown"
}

fn route_intent(route: &str) -> theme::Intent {
    match route {
        "eeg" => theme::Intent::Success,
        "motion" => theme::Intent::Info,
        "auxiliary" => theme::Intent::Warning,
        _ => theme::Intent::Muted,
    }
}

fn drop_rate_intent(drop_rate_pct: Option<f32>) -> theme::Intent {
    match drop_rate_pct.unwrap_or_default() {
        value if value < 1.0 => theme::Intent::Success,
        value if value < 5.0 => theme::Intent::Warning,
        _ => theme::Intent::Danger,
    }
}

fn staleness_intent(staleness_ms: Option<u64>) -> theme::Intent {
    match staleness_ms {
        Some(value) if value <= 250 => theme::Intent::Success,
        Some(value) if value <= 1_000 => theme::Intent::Warning,
        Some(_) => theme::Intent::Danger,
        None => theme::Intent::Muted,
    }
}
