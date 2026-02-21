//! # Extensions Screen
//!
//! Lists discovered extensions for all four pipeline slots (outlet, device,
//! signal preprocessing, decoder) and supports on-demand rescan.

use eframe::egui;

use neurohid_core::extension_registry::{default_extension_paths, ExtensionRegistry};

use crate::state::HubState;
use crate::theme;

/// Extension kind label for display.
fn kind_label(kind: &str) -> &'static str {
    match kind {
        "outlet" => "Outlet",
        "device" => "Device",
        "signal_preprocessing" => "Signal preprocessing",
        "decoder" => "Decoder",
        _ => "Extension",
    }
}

pub struct ExtensionsScreen {
    /// Cached list after last scan. (kind, name, path display)
    entries: Vec<(String, String, String)>,
    /// Last scan error message if any.
    scan_error: Option<String>,
    /// True when a rescan was requested this frame.
    rescan_requested: bool,
    /// True after at least one scan has completed (avoids rescanning every frame when empty).
    scanned_once: bool,
}

impl Default for ExtensionsScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtensionsScreen {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            scan_error: None,
            rescan_requested: false,
            scanned_once: false,
        }
    }

    fn run_scan(&mut self) {
        self.scan_error = None;
        let paths = default_extension_paths();
        let mut registry = ExtensionRegistry::new(paths);
        match registry.scan() {
            Ok(()) => {
                self.entries.clear();
                for e in registry.list_outlets() {
                    self.entries.push((
                        "outlet".to_string(),
                        e.name,
                        e.path.display().to_string(),
                    ));
                }
                for e in registry.list_devices() {
                    self.entries.push((
                        "device".to_string(),
                        e.name,
                        e.path.display().to_string(),
                    ));
                }
                for e in registry.list_signal_preprocessors() {
                    self.entries.push((
                        "signal_preprocessing".to_string(),
                        e.name,
                        e.path.display().to_string(),
                    ));
                }
                for e in registry.list_decoders() {
                    self.entries.push((
                        "decoder".to_string(),
                        e.name,
                        e.path.display().to_string(),
                    ));
                }
            }
            Err(e) => {
                self.scan_error = Some(e.to_string());
                self.entries.clear();
            }
        }
        self.scanned_once = true;
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        _state: &HubState,
        _runtime: &tokio::runtime::Runtime,
    ) {
        theme::page_header(
            ui,
            "Extensions",
            "Discovered pipeline extensions (outlet, device, signal preprocessing, decoder). \
             Use Rescan after adding or removing extensions.",
        );

        // Run scan on first show or when Rescan was clicked.
        if self.rescan_requested || !self.scanned_once {
            self.rescan_requested = false;
            self.run_scan();
        }

        theme::card_frame(ui).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let refresh_clicked = theme::action_button(
                    ui,
                    "Rescan",
                    true,
                    theme::ButtonTone::Primary,
                );
                if refresh_clicked {
                    self.rescan_requested = true;
                }
            });
            if let Some(ref err) = self.scan_error {
                theme::status_chip(ui, err, theme::Intent::Warning);
            }
        });

        ui.add_space(10.0);

        theme::card_frame(ui).show(ui, |ui| {
            ui.label(
                egui::RichText::new("Discovered extensions (all kinds)")
                    .small()
                    .weak(),
            );
            if self.entries.is_empty() && self.scan_error.is_none() {
                theme::status_chip(
                    ui,
                    "No extensions found. Click Rescan or add extensions to the discovery path.",
                    theme::Intent::Muted,
                );
            } else {
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    ui.text_style_height(&egui::TextStyle::Body),
                    self.entries.len(),
                    |ui, row_range| {
                        for (kind, name, path) in self.entries[row_range].iter() {
                            ui.horizontal(|ui| {
                                ui.label(kind_label(kind));
                                ui.label(name);
                                ui.label(
                                    egui::RichText::new(path.as_str())
                                        .small()
                                        .weak(),
                                );
                            });
                        }
                    },
                );
            }
        });
    }
}
