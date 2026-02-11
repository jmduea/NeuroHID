//! # Widget System
//!
//! The widget trait, widget identifier enum, and factory for instantiating
//! widgets by ID. Widgets are the building blocks of the Visualization screen.

pub mod action_preview;
pub mod band_power;
pub mod decoder_monitor;
pub mod fft_plot;
pub mod signal_quality;
pub mod time_series;

use crate::data_bus::DataBus;
use crate::state::ServiceSnapshot;
use eframe::egui;
use neurohid_types::signal::Sample;
use std::collections::VecDeque;

/// Unique identifier for each widget type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum WidgetId {
    TimeSeries,
    FftPlot,
    BandPower,
    SignalQuality,
    DecoderMonitor,
    ActionPreview,
}

impl WidgetId {
    /// All available widget types.
    pub const ALL: &'static [WidgetId] = &[
        WidgetId::TimeSeries,
        WidgetId::FftPlot,
        WidgetId::BandPower,
        WidgetId::SignalQuality,
        WidgetId::DecoderMonitor,
        WidgetId::ActionPreview,
    ];

    /// Human-readable label for display in dropdown menus.
    pub fn label(&self) -> &'static str {
        match self {
            WidgetId::TimeSeries => "Time Series",
            WidgetId::FftPlot => "FFT Plot",
            WidgetId::BandPower => "Band Power",
            WidgetId::SignalQuality => "Signal Quality",
            WidgetId::DecoderMonitor => "Decoder Monitor",
            WidgetId::ActionPreview => "Action Preview",
        }
    }

    /// The LSL stream types this widget should consume.
    /// Widgets that don't read raw samples (e.g. ActionPreview) return
    /// an empty slice — they will fall back to the full sample buffer.
    pub fn accepted_stream_types(&self) -> &'static [&'static str] {
        match self {
            WidgetId::TimeSeries => &["EEG"],
            WidgetId::FftPlot => &["EEG"],
            WidgetId::BandPower => &["EEG"],
            WidgetId::SignalQuality => &["EEG"],
            WidgetId::DecoderMonitor => &["EEG"],
            WidgetId::ActionPreview => &[], // reads actions, not samples
        }
    }
}

/// Context passed to widgets each frame.
pub struct WidgetContext<'a> {
    pub bus: &'a DataBus,
    pub snapshot: &'a ServiceSnapshot,
}

impl<'a> WidgetContext<'a> {
    /// Get the samples buffer filtered to the stream types accepted by
    /// the given widget. Returns a reference to a per-source ring buffer
    /// when a matching stream is found, otherwise falls back to the flat
    /// sample buffer so single-stream / mock setups keep working.
    pub fn samples_for(&self, widget_id: WidgetId) -> &'a VecDeque<Sample> {
        let types = widget_id.accepted_stream_types();
        if types.is_empty() {
            return &self.bus.samples;
        }
        self.bus
            .samples_for_type(types, &self.snapshot.discovered_streams)
    }
}

/// The trait implemented by all visualization widgets.
pub trait Widget {
    /// The widget's unique identifier.
    fn id(&self) -> WidgetId;

    /// Human-readable title.
    fn title(&self) -> &str;

    /// Render the widget into the given UI area.
    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>);
}

/// Factory: create a boxed widget from its ID.
pub fn create_widget(id: WidgetId) -> Box<dyn Widget> {
    match id {
        WidgetId::TimeSeries => Box::new(time_series::TimeSeriesWidget::new()),
        WidgetId::FftPlot => Box::new(fft_plot::FftPlotWidget::new()),
        WidgetId::BandPower => Box::new(band_power::BandPowerWidget::new()),
        WidgetId::SignalQuality => Box::new(signal_quality::SignalQualityWidget::new()),
        WidgetId::DecoderMonitor => Box::new(decoder_monitor::DecoderMonitorWidget::new()),
        WidgetId::ActionPreview => Box::new(action_preview::ActionPreviewWidget::new()),
    }
}
