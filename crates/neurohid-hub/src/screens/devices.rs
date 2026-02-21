//! # Devices Screen
//!
//! **One place** for device/stream discovery, connection, and health. Shows
//! ControlSnapshot.discovered_streams with connection status (connected/not),
//! stream identity, and stream health (channel_quality, integrity_state, etc.).
//! Rescan, ConnectStream, and DisconnectStream are dispatched via ServiceManager.

use std::collections::BTreeMap;

use eframe::egui;

use crate::service_manager::ServiceManager;
use crate::state::HubState;
use crate::theme;
use neurohid_types::device::DiscoveredStream;

pub struct DevicesScreen;

impl Default for DevicesScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl DevicesScreen {
    pub fn new() -> Self {
        Self
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        state: &HubState,
        service_manager: &mut ServiceManager,
    ) {
        theme::page_header(
            ui,
            "Devices",
            "One place to discover, connect, and monitor streams — connection status and health here",
        );

        let snap = &state.service_snapshot;

        if !snap.running {
            theme::card_frame(ui).show(ui, |ui| {
                theme::status_chip(ui, "Service stopped", theme::Intent::Warning);
                theme::status_chip(
                    ui,
                    "Start service to discover/connect LSL streams",
                    theme::Intent::Info,
                );
                theme::status_chip(ui, "Use Dashboard to start service", theme::Intent::Muted);
            });
            return;
        }

        let total_streams = snap.discovered_streams.len();
        let connected_streams = snap
            .discovered_streams
            .iter()
            .filter(|stream| stream.connected)
            .count();
        let route_total = snap.routed_eeg_streams
            + snap.routed_motion_streams
            + snap.routed_auxiliary_streams
            + snap.routed_unknown_streams;

        theme::card_frame(ui).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                theme::status_chip(
                    ui,
                    &format!("Streams {}/{}", connected_streams, total_streams),
                    if connected_streams > 0 {
                        theme::Intent::Success
                    } else {
                        theme::Intent::Muted
                    },
                );
                theme::status_chip(
                    ui,
                    &format!("Signal {:.0}%", snap.signal_quality * 100.0),
                    if snap.signal_quality > 0.7 {
                        theme::Intent::Success
                    } else if snap.signal_quality > 0.5 {
                        theme::Intent::Warning
                    } else {
                        theme::Intent::Danger
                    },
                );
                if route_total > 0 {
                    theme::status_chip(ui, &format!("Routes {}", route_total), theme::Intent::Info);
                }
                if snap.device_connected {
                    theme::status_chip(ui, "Device linked", theme::Intent::Info);
                } else {
                    theme::status_chip(ui, "Device idle", theme::Intent::Muted);
                }
                theme::status_chip(ui, "LSL-first scope", theme::Intent::Info);
            });
        });
        ui.add_space(8.0);

        // Header with rescan button
        ui.horizontal(|ui| {
            ui.heading("Available Streams (LSL-first)");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::action_button(ui, "Rescan", true, theme::ButtonTone::Primary) {
                    service_manager.rescan_streams();
                }
            });
        });
        ui.add_space(8.0);

        if snap.discovered_streams.is_empty() {
            theme::card_frame(ui).show(ui, |ui| {
                theme::status_chip(ui, "No streams found", theme::Intent::Warning);
                ui.add_space(4.0);
                theme::status_chip(
                    ui,
                    "Ensure device software is pushing to LSL; use Rescan to check manually",
                    theme::Intent::Warning,
                );
                theme::status_chip(
                    ui,
                    "Serial/BrainFlow parity is planned in later phases",
                    theme::Intent::Muted,
                );
                theme::status_chip(
                    ui,
                    "Auto-rescan runs when none are connected",
                    theme::Intent::Muted,
                );
            });
        } else {
            // Group streams by source_id. Streams sharing a source_id come
            // from the same physical device and are rendered under a single
            // collapsible header. Streams without a source_id (None) are
            // rendered as standalone cards.
            let mut groups: BTreeMap<Option<String>, Vec<&DiscoveredStream>> = BTreeMap::new();
            for stream in &snap.discovered_streams {
                groups
                    .entry(stream.source_id.clone())
                    .or_default()
                    .push(stream);
            }

            for (source_id, streams) in &groups {
                match source_id {
                    Some(src_id) if streams.len() > 1 => {
                        // Multi-stream device group
                        Self::show_device_group(ui, src_id, streams, service_manager);
                    }
                    _ => {
                        // Standalone stream(s) — render as individual cards
                        for stream in streams {
                            Self::show_stream_card(ui, stream, service_manager);
                            ui.add_space(4.0);
                        }
                    }
                }
            }
        }

        // Connected stream detail section
        let connected_count = connected_streams;

        if connected_count > 0 {
            ui.add_space(12.0);
            theme::card_frame(ui).show(ui, |ui| {
                ui.heading("Signal Quality");
                ui.add_space(8.0);

                // Overall quality bar
                let quality = snap.signal_quality;
                let quality_intent = if quality > 0.7 {
                    theme::Intent::Success
                } else if quality > 0.5 {
                    theme::Intent::Warning
                } else {
                    theme::Intent::Danger
                };

                ui.horizontal(|ui| {
                    theme::status_chip(
                        ui,
                        &format!("Overall {:.0}%", quality * 100.0),
                        quality_intent,
                    );
                    let _ = theme::progress_bar(ui, quality, ui.available_width().min(260.0));
                });

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(format!("Aggregate across {} streams", connected_count))
                        .small()
                        .color(egui::Color32::GRAY),
                );
            });
        }
    }

    /// Render a collapsible device group containing multiple streams.
    fn show_device_group(
        ui: &mut egui::Ui,
        source_id: &str,
        streams: &[&DiscoveredStream],
        service_manager: &mut ServiceManager,
    ) {
        let connected_count = streams.iter().filter(|s| s.connected).count();
        let total = streams.len();
        let all_connected = connected_count == total;
        let any_connected = connected_count > 0;

        // Derive a display name from the stream names. If all stream names
        // share a common prefix (e.g., "EmotivEEG", "EmotivMotion"), use
        // the prefix. Otherwise fall back to the source_id.
        let device_label = derive_device_label(streams, source_id);

        // Battery: take the first non-None battery reading from any stream in the group
        let battery = streams.iter().find_map(|s| s.battery_percent);
        let average_drop_pct = {
            let values: Vec<f32> = streams.iter().filter_map(|s| s.drop_rate_pct).collect();
            if values.is_empty() {
                None
            } else {
                Some(values.iter().sum::<f32>() / values.len() as f32)
            }
        };
        let worst_staleness_ms = streams.iter().filter_map(|s| s.last_sample_age_ms).max();
        let degraded_streams = streams
            .iter()
            .filter(|s| s.integrity_state.as_deref() == Some("degraded"))
            .count();

        // Device-level status indicator
        let status_text = if all_connected {
            "All connected"
        } else if any_connected {
            "Partially connected"
        } else {
            "Available"
        };

        theme::card_frame(ui).show(ui, |ui| {
            // Device header — vertical card layout
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&device_label).strong().size(15.0));
                let status_intent = if all_connected {
                    theme::Intent::Success
                } else if any_connected {
                    theme::Intent::Warning
                } else {
                    theme::Intent::Muted
                };
                theme::status_chip(ui, status_text, status_intent);
                if let Some(bat) = battery {
                    let battery_intent = if bat > 50 {
                        theme::Intent::Success
                    } else if bat > 20 {
                        theme::Intent::Warning
                    } else {
                        theme::Intent::Danger
                    };
                    theme::status_chip(ui, &format!("Battery {}%", bat), battery_intent);
                }
                if let Some(drop_pct) = average_drop_pct {
                    theme::status_chip(
                        ui,
                        &format!("Avg drop {:.1}%", drop_pct),
                        drop_rate_intent(Some(drop_pct)),
                    );
                }
                if let Some(staleness_ms) = worst_staleness_ms {
                    theme::status_chip(
                        ui,
                        &format!("Worst age {}ms", staleness_ms),
                        staleness_intent(Some(staleness_ms)),
                    );
                }
                if degraded_streams > 0 {
                    theme::status_chip(
                        ui,
                        &format!("Integrity degraded {}", degraded_streams),
                        theme::Intent::Warning,
                    );
                }
            });
            ui.label(
                egui::RichText::new(format!(
                    "{}/{} streams \u{2022} {}",
                    connected_count, total, status_text
                ))
                .small()
                .color(egui::Color32::LIGHT_GRAY),
            );
            ui.add_space(4.0);

            // Connect All / Disconnect All buttons
            ui.horizontal(|ui| {
                if all_connected {
                    if theme::action_button(ui, "Disconnect All", true, theme::ButtonTone::Ghost) {
                        let ids: Vec<&str> = streams.iter().map(|s| s.id.as_str()).collect();
                        service_manager.disconnect_streams(&ids);
                        ui.ctx().request_repaint();
                    }
                } else {
                    if theme::action_button(ui, "Connect All", true, theme::ButtonTone::Primary) {
                        let ids: Vec<&str> = streams
                            .iter()
                            .filter(|s| !s.connected)
                            .map(|s| s.id.as_str())
                            .collect();
                        service_manager.connect_streams(&ids);
                        ui.ctx().request_repaint();
                    }
                    if any_connected
                        && theme::action_button(
                            ui,
                            "Disconnect All",
                            true,
                            theme::ButtonTone::Ghost,
                        )
                    {
                        let ids: Vec<&str> = streams
                            .iter()
                            .filter(|s| s.connected)
                            .map(|s| s.id.as_str())
                            .collect();
                        service_manager.disconnect_streams(&ids);
                        ui.ctx().request_repaint();
                    }
                }
            });

            // Collapsible stream list
            let header_id = ui.make_persistent_id(source_id);
            egui::collapsing_header::CollapsingState::load_with_default_open(
                ui.ctx(),
                header_id,
                true,
            )
            .show_header(ui, |ui| {
                ui.label(
                    egui::RichText::new(format!("{} streams", total))
                        .small()
                        .color(egui::Color32::GRAY),
                );
            })
            .body(|ui| {
                for stream in streams {
                    Self::show_stream_entry(ui, stream, service_manager);
                    ui.add_space(2.0);
                }
            });
        });
        ui.add_space(4.0);
    }

    /// Render a single stream as an indented entry inside a device group.
    fn show_stream_entry(
        ui: &mut egui::Ui,
        stream: &DiscoveredStream,
        service_manager: &mut ServiceManager,
    ) {
        let (status, status_intent) = if stream.connected {
            ("Connected", theme::Intent::Success)
        } else {
            ("Available", theme::Intent::Muted)
        };

        ui.horizontal(|ui| {
            ui.add_space(16.0); // indent
            ui.label(egui::RichText::new(&stream.name).strong());
            theme::status_chip(ui, status, status_intent);
        });
        ui.horizontal(|ui| {
            ui.add_space(32.0); // indent metadata
            ui.label(
                egui::RichText::new(&stream.stream_type)
                    .small()
                    .color(egui::Color32::LIGHT_GRAY),
            );
            ui.label(
                egui::RichText::new(format!(
                    "{}ch @ {:.0} Hz",
                    stream.channel_count, stream.sample_rate
                ))
                .small()
                .color(egui::Color32::LIGHT_GRAY),
            );
        });
        show_stream_runtime_chips(ui, stream, 32.0);
        show_preprocessing_and_integrity(ui, stream, 32.0);
        ui.horizontal(|ui| {
            ui.add_space(32.0); // indent button
            if stream.connected {
                if theme::action_button(ui, "Disconnect", true, theme::ButtonTone::Ghost) {
                    service_manager.disconnect_stream(&stream.id);
                    ui.ctx().request_repaint();
                }
            } else if theme::action_button(ui, "Connect", true, theme::ButtonTone::Primary) {
                service_manager.connect_stream(&stream.id);
                ui.ctx().request_repaint();
            }
        });

        // Per-channel quality bars for connected streams
        if stream.connected
            && let Some(qualities) = &stream.channel_quality
        {
            ui.horizontal(|ui| {
                ui.add_space(32.0); // indent quality bars
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Channel Quality")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    for (i, &q) in qualities.iter().enumerate() {
                        let q_intent = if q > 0.7 {
                            theme::Intent::Success
                        } else if q > 0.4 {
                            theme::Intent::Warning
                        } else {
                            theme::Intent::Danger
                        };
                        let _ = theme::progress_bar(ui, q, ui.available_width());
                        theme::status_chip(ui, &format!("Ch{} {:.0}%", i, q * 100.0), q_intent);
                    }
                });
            });
        }
    }

    /// Render a standalone stream as an independent card (no device grouping).
    fn show_stream_card(
        ui: &mut egui::Ui,
        stream: &DiscoveredStream,
        service_manager: &mut ServiceManager,
    ) {
        theme::card_frame(ui).show(ui, |ui| {
            // Status + stream name + battery
            let (status, status_intent) = if stream.connected {
                ("Connected", theme::Intent::Success)
            } else {
                ("Available", theme::Intent::Muted)
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&stream.name).strong());
                theme::status_chip(ui, status, status_intent);
                if let Some(bat) = stream.battery_percent {
                    let battery_intent = if bat > 50 {
                        theme::Intent::Success
                    } else if bat > 20 {
                        theme::Intent::Warning
                    } else {
                        theme::Intent::Danger
                    };
                    theme::status_chip(ui, &format!("Battery {}%", bat), battery_intent);
                }
            });
            // Stream metadata
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&stream.stream_type)
                        .small()
                        .color(egui::Color32::LIGHT_GRAY),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{}ch @ {:.0} Hz",
                        stream.channel_count, stream.sample_rate
                    ))
                    .small()
                    .color(egui::Color32::LIGHT_GRAY),
                );
            });
            show_stream_runtime_chips(ui, stream, 0.0);
            show_preprocessing_and_integrity(ui, stream, 0.0);
            ui.add_space(4.0);
            // Connect/Disconnect button
            if stream.connected {
                if theme::action_button(ui, "Disconnect", true, theme::ButtonTone::Ghost) {
                    service_manager.disconnect_stream(&stream.id);
                    ui.ctx().request_repaint();
                }
            } else if theme::action_button(ui, "Connect", true, theme::ButtonTone::Primary) {
                service_manager.connect_stream(&stream.id);
                ui.ctx().request_repaint();
            }

            // Per-channel quality bars for connected streams
            if stream.connected
                && let Some(qualities) = &stream.channel_quality
            {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Channel Quality")
                        .small()
                        .color(egui::Color32::GRAY),
                );
                for (i, &q) in qualities.iter().enumerate() {
                    let q_intent = if q > 0.7 {
                        theme::Intent::Success
                    } else if q > 0.4 {
                        theme::Intent::Warning
                    } else {
                        theme::Intent::Danger
                    };
                    let _ = theme::progress_bar(ui, q, ui.available_width());
                    theme::status_chip(ui, &format!("Ch{} {:.0}%", i, q * 100.0), q_intent);
                }
            }
        });
    }
}

/// Derive a human-friendly device label from the stream names in a group.
///
/// Tries to find a common prefix among stream names (e.g., "Emotiv" from
/// "EmotivEEG", "EmotivMotion", "EmotivEQ"). Falls back to the raw
/// `source_id` if no meaningful prefix is found.
pub(crate) fn derive_device_label(streams: &[&DiscoveredStream], source_id: &str) -> String {
    if streams.is_empty() {
        return source_id.to_string();
    }

    let names: Vec<&str> = streams.iter().map(|s| s.name.as_str()).collect();

    // Find common prefix across all names
    let first = names[0];
    let mut prefix_len = first.len();
    for name in &names[1..] {
        prefix_len = first
            .chars()
            .zip(name.chars())
            .take(prefix_len)
            .take_while(|(a, b)| a == b)
            .count();
    }

    // Only use the prefix if it's at least 3 characters (meaningful)
    if prefix_len >= 3 {
        let prefix = &first[..first
            .char_indices()
            .nth(prefix_len)
            .map(|(i, _)| i)
            .unwrap_or(first.len())];
        // Trim trailing non-alphanumeric chars (e.g., "Emotiv_" → "Emotiv")
        let trimmed = prefix.trim_end_matches(|c: char| !c.is_alphanumeric());
        if trimmed.len() >= 3 {
            return trimmed.to_string();
        }
    }

    // Fallback: use source_id (possibly truncated for display)
    if source_id.len() > 24 {
        format!("{}…", &source_id[..24])
    } else {
        source_id.to_string()
    }
}

fn stream_route_hint(stream: &DiscoveredStream) -> &'static str {
    let name = stream.name.to_ascii_lowercase();
    let stream_type = stream.stream_type.to_ascii_lowercase();
    let id = stream.id.to_ascii_lowercase();
    let combined = format!("{stream_type} {name} {id}");

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

fn show_stream_runtime_chips(ui: &mut egui::Ui, stream: &DiscoveredStream, indent_px: f32) {
    ui.horizontal_wrapped(|ui| {
        if indent_px > 0.0 {
            ui.add_space(indent_px);
        }

        let route = stream_route_hint(stream);
        theme::status_chip(ui, &format!("Route {}", route), route_intent(route));

        if let Some(effective_hz) = stream.effective_sample_rate_hz {
            theme::status_chip(
                ui,
                &format!("Eff {:.1} Hz", effective_hz),
                theme::Intent::Info,
            );
        }

        theme::status_chip(
            ui,
            &format!(
                "Drop {}",
                stream
                    .drop_rate_pct
                    .map(|value| format!("{value:.1}%"))
                    .unwrap_or_else(|| "n/a".to_string())
            ),
            drop_rate_intent(stream.drop_rate_pct),
        );

        theme::status_chip(
            ui,
            &format!(
                "Age {}",
                stream
                    .last_sample_age_ms
                    .map(|value| format!("{value}ms"))
                    .unwrap_or_else(|| "n/a".to_string())
            ),
            staleness_intent(stream.last_sample_age_ms),
        );
    });
}

fn show_preprocessing_and_integrity(ui: &mut egui::Ui, stream: &DiscoveredStream, indent_px: f32) {
    ui.horizontal_wrapped(|ui| {
        if indent_px > 0.0 {
            ui.add_space(indent_px);
        }

        if let Some(summary) = &stream.preprocessing_summary {
            theme::status_chip(ui, &format!("Preproc {}", summary), theme::Intent::Muted);
        } else {
            theme::status_chip(ui, "Preproc pending", theme::Intent::Muted);
        }

        let integrity = stream.integrity_state.as_deref().unwrap_or("unknown");
        let integrity_intent = match integrity {
            "ok" => theme::Intent::Success,
            "degraded" => theme::Intent::Warning,
            _ => theme::Intent::Muted,
        };
        theme::status_chip(ui, &format!("Integrity {integrity}"), integrity_intent);
    });
}

#[cfg(test)]
mod tests {
    use super::{
        DiscoveredStream, derive_device_label, drop_rate_intent, staleness_intent,
        stream_route_hint,
    };
    use crate::theme::Intent;

    fn make_stream(name: &str, stream_type: &str, channel_count: i32) -> DiscoveredStream {
        DiscoveredStream {
            id: format!("src::{name}"),
            name: name.to_string(),
            stream_type: stream_type.to_string(),
            channel_count,
            sample_rate: 128.0,
            connected: true,
            battery_percent: None,
            channel_quality: None,
            source_id: Some("src".to_string()),
            effective_sample_rate_hz: None,
            samples_received: None,
            samples_dropped: None,
            drop_rate_pct: None,
            last_sample_age_ms: None,
            preprocessing_summary: None,
            integrity_state: None,
        }
    }

    #[test]
    fn route_hint_detects_eeg_motion_and_auxiliary_streams() {
        let eeg = make_stream("EmotivEEG", "EEG", 5);
        let motion = make_stream("EmotivMotion", "Motion", 3);
        let auxiliary = make_stream("EmotivMentalCommands", "MentalCommand", 1);

        assert_eq!(stream_route_hint(&eeg), "eeg");
        assert_eq!(stream_route_hint(&motion), "motion");
        assert_eq!(stream_route_hint(&auxiliary), "auxiliary");
    }

    #[test]
    fn intent_thresholds_for_drop_and_staleness_are_stable() {
        assert_eq!(drop_rate_intent(Some(0.2)), Intent::Success);
        assert_eq!(drop_rate_intent(Some(2.0)), Intent::Warning);
        assert_eq!(drop_rate_intent(Some(8.0)), Intent::Danger);
        assert_eq!(staleness_intent(Some(120)), Intent::Success);
        assert_eq!(staleness_intent(Some(900)), Intent::Warning);
        assert_eq!(staleness_intent(Some(5_000)), Intent::Danger);
    }

    #[test]
    fn derive_device_label_uses_shared_prefix_when_available() {
        let eeg = make_stream("EmotivEEG", "EEG", 5);
        let motion = make_stream("EmotivMotion", "Motion", 3);
        let streams = vec![&eeg, &motion];
        assert_eq!(derive_device_label(&streams, "src"), "Emotiv");
    }
}
