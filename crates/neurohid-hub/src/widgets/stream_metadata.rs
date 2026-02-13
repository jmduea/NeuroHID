//! # Stream Metadata Widget
//!
//! Always-on metadata surface for discovered and connected streams.

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
            ui.label(egui::RichText::new("No streams discovered").weak());
            return;
        }

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
                    ui.label(
                        s.battery_percent
                            .map(|v| format!("{}%", v))
                            .unwrap_or_else(|| "-".to_string()),
                    );
                    ui.label(if s.connected { "yes" } else { "no" });
                    ui.end_row();
                }
            });

        ui.add_space(8.0);
        ui.label(egui::RichText::new("Source IDs").small().strong());
        for s in streams {
            ui.label(
                egui::RichText::new(format!(
                    "{} -> {}",
                    s.name,
                    s.source_id.clone().unwrap_or_else(|| "<none>".to_string())
                ))
                .small()
                .weak(),
            );
        }
    }
}
