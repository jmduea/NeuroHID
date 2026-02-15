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
            .min_col_width(70.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Name").strong());
                ui.label(egui::RichText::new("Type").strong());
                ui.label(egui::RichText::new("Rate").strong());
                ui.label(egui::RichText::new("Ch").strong());
                ui.label(egui::RichText::new("Battery").strong());
                ui.label(egui::RichText::new("Connected").strong());
                ui.end_row();

                for s in streams {
                    ui.label(&s.name);
                    ui.label(&s.stream_type);
                    ui.label(format!("{:.0} Hz", s.sample_rate));
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
                    if s.connected {
                        theme::status_chip(ui, "yes", theme::Intent::Success);
                    } else {
                        theme::status_chip(ui, "no", theme::Intent::Muted);
                    }
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
