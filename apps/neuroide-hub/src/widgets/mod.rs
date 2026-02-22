//! # Widget System
//!
//! The widget trait, widget identifier enum, and factory for instantiating
//! widgets by ID. Widgets are the building blocks of the Visualization screen.

pub mod accelerometer;
pub mod action_preview;
pub mod band_power;
pub mod channel_meta;
pub mod decoder_monitor;
pub mod fft_plot;
pub mod focus;
pub mod headplot;
pub mod signal_quality;
pub mod spectrogram;
pub mod stream_metadata;
pub mod time_series;

use crate::data_bus::DataBus;
use crate::state::ServiceSnapshot;
use eframe::egui;
use neurohid_types::device::DiscoveredStream;
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
    Accelerometer,
    Spectrogram,
    Focus,
    Headplot,
    StreamMetadata,
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
        WidgetId::Accelerometer,
        WidgetId::Spectrogram,
        WidgetId::Focus,
        WidgetId::Headplot,
        WidgetId::StreamMetadata,
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
            WidgetId::Accelerometer => "Accelerometer",
            WidgetId::Spectrogram => "Spectrogram",
            WidgetId::Focus => "Focus",
            WidgetId::Headplot => "Headplot",
            WidgetId::StreamMetadata => "Stream Metadata",
        }
    }

    /// The LSL stream types this widget should consume.
    /// Widgets that don't read raw samples (e.g. ActionPreview) return
    /// an empty slice — they will fall back to the full sample buffer.
    pub fn accepted_stream_types(&self) -> &'static [&'static str] {
        match self {
            WidgetId::TimeSeries => &["EEG"],
            WidgetId::FftPlot => &["EEG"],
            WidgetId::BandPower => &["EEG", "FFT"],
            WidgetId::SignalQuality => &["EEG"],
            WidgetId::DecoderMonitor => &["EEG"],
            WidgetId::ActionPreview => &[], // reads actions, not samples
            WidgetId::Accelerometer => &["Motion", "ACC"],
            WidgetId::Spectrogram => &["EEG"],
            WidgetId::Focus => &["EEG", "FFT"],
            WidgetId::Headplot => &["EEG"],
            WidgetId::StreamMetadata => &[],
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

    /// Get samples from a specific stream type (e.g. "Quality").
    /// Returns `None` if no matching stream data is available.
    pub fn samples_for_type(&self, stream_type: &str) -> Option<&'a VecDeque<Sample>> {
        let types = &[stream_type];
        let buf = self
            .bus
            .samples_for_type(types, &self.snapshot.discovered_streams);
        // samples_for_type falls back to the flat buffer — we only want
        // a real match, so check that we didn't get the flat fallback.
        if std::ptr::eq(buf, &self.bus.samples) {
            None
        } else {
            Some(buf)
        }
    }

    /// Resolve the channel count from the first discovered stream matching
    /// the given types. Returns `None` if no matching stream is found,
    /// in which case callers should fall back to per-sample detection.
    pub fn channel_count_for(&self, stream_types: &[&str]) -> Option<usize> {
        for ds in &self.snapshot.discovered_streams {
            let ds_type_prefix = ds.stream_type.split('/').next().unwrap_or("");
            if stream_types
                .iter()
                .any(|st| ds_type_prefix.eq_ignore_ascii_case(st))
                && ds.channel_count > 0
            {
                return Some(ds.channel_count as usize);
            }
        }
        None
    }

    /// Get samples for a specific discovered stream/source id.
    pub fn samples_for_source(&self, source_id: &str) -> Option<&'a VecDeque<Sample>> {
        self.bus.samples_by_source.get(source_id)
    }

    /// Get discovered streams whose type matches the given widget's accepted
    /// source stream types.
    pub fn candidate_sources_for(&self, widget_id: WidgetId) -> Vec<&'a DiscoveredStream> {
        let accepted = widget_id.accepted_stream_types();
        if accepted.is_empty() {
            return Vec::new();
        }

        self.snapshot
            .discovered_streams
            .iter()
            .filter(|stream| {
                let type_prefix = stream.stream_type.split('/').next().unwrap_or("");
                accepted
                    .iter()
                    .any(|wanted| type_prefix.eq_ignore_ascii_case(wanted))
            })
            .collect()
    }

    /// Resolve samples for a widget, optionally forcing a specific source.
    pub fn samples_for_widget_source(
        &self,
        widget_id: WidgetId,
        source_id: Option<&str>,
    ) -> &'a VecDeque<Sample> {
        if let Some(id) = source_id
            && let Some(samples) = self.samples_for_source(id)
        {
            return samples;
        }
        self.samples_for(widget_id)
    }

    /// Resolve channel count from a discovered source id.
    pub fn channel_count_for_source(&self, source_id: &str) -> Option<usize> {
        self.snapshot
            .discovered_streams
            .iter()
            .find(|s| s.id == source_id && s.channel_count > 0)
            .map(|s| s.channel_count as usize)
    }

    /// Resolve nominal sample rate from a discovered source id.
    pub fn sample_rate_for_source(&self, source_id: &str) -> Option<f64> {
        self.snapshot
            .discovered_streams
            .iter()
            .find(|s| s.id == source_id && s.sample_rate > 0.0)
            .map(|s| s.effective_sample_rate_hz.unwrap_or(s.sample_rate))
    }
}

/// The trait implemented by all visualization widgets.
pub trait Widget {
    /// The widget's unique identifier.
    fn id(&self) -> WidgetId;

    /// Human-readable title.
    fn title(&self) -> &str;

    /// Render the widget into the given UI area.
    ///
    /// `pane_index` is the index of the pane hosting this widget instance.
    /// Widgets must incorporate it into any egui IDs (ComboBox, Grid, tooltips)
    /// to avoid ID clashes when the same widget type appears in multiple panes.
    fn show(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>, pane_index: usize);
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
        WidgetId::Accelerometer => Box::new(accelerometer::AccelerometerWidget::new()),
        WidgetId::Spectrogram => Box::new(spectrogram::SpectrogramWidget::new()),
        WidgetId::Focus => Box::new(focus::FocusWidget::new()),
        WidgetId::Headplot => Box::new(headplot::HeadplotWidget::new()),
        WidgetId::StreamMetadata => Box::new(stream_metadata::StreamMetadataWidget::new()),
    }
}
