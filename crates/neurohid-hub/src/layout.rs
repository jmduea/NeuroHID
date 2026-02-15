//! # Layout Engine
//!
//! Manages the multi-pane widget layout for the Visualization screen.
//! Users choose a layout configuration (e.g., 2x2, 1+2), assign
//! a widget to each pane, and drag/resize panes via `egui_dock`.

use crate::theme;
use crate::widgets::{Widget, WidgetContext, WidgetId, create_widget};
use eframe::egui;
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};
use neurohid_types::config::UiConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LayoutConfig {
    Single,
    TwoColumns,
    TwoRows,
    Grid2x2,
    OneLeftTwoRight,
    TwoLeftOneRight,
}

impl LayoutConfig {
    pub const ALL: &'static [LayoutConfig] = &[
        LayoutConfig::Single,
        LayoutConfig::TwoColumns,
        LayoutConfig::TwoRows,
        LayoutConfig::Grid2x2,
        LayoutConfig::OneLeftTwoRight,
        LayoutConfig::TwoLeftOneRight,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            LayoutConfig::Single => "1 Pane",
            LayoutConfig::TwoColumns => "2 Columns",
            LayoutConfig::TwoRows => "2 Rows",
            LayoutConfig::Grid2x2 => "2x2 Grid",
            LayoutConfig::OneLeftTwoRight => "1 + 2",
            LayoutConfig::TwoLeftOneRight => "2 + 1",
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            LayoutConfig::Single => "single",
            LayoutConfig::TwoColumns => "two_columns",
            LayoutConfig::TwoRows => "two_rows",
            LayoutConfig::Grid2x2 => "grid2x2",
            LayoutConfig::OneLeftTwoRight => "one_left_two_right",
            LayoutConfig::TwoLeftOneRight => "two_left_one_right",
        }
    }

    pub fn from_key(value: &str) -> Option<Self> {
        match value {
            "single" => Some(LayoutConfig::Single),
            "two_columns" => Some(LayoutConfig::TwoColumns),
            "two_rows" => Some(LayoutConfig::TwoRows),
            "grid2x2" => Some(LayoutConfig::Grid2x2),
            "one_left_two_right" => Some(LayoutConfig::OneLeftTwoRight),
            "two_left_one_right" => Some(LayoutConfig::TwoLeftOneRight),
            _ => None,
        }
    }

    pub fn pane_count(&self) -> usize {
        match self {
            LayoutConfig::Single => 1,
            LayoutConfig::TwoColumns | LayoutConfig::TwoRows => 2,
            LayoutConfig::Grid2x2 => 4,
            LayoutConfig::OneLeftTwoRight | LayoutConfig::TwoLeftOneRight => 3,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct Pane {
    slot: usize,
    widget_id: WidgetId,
}

impl Pane {
    fn new(slot: usize, widget_id: WidgetId) -> Self {
        Self { slot, widget_id }
    }
}

pub struct PersistedLayoutState {
    pub layout_preset: String,
    pub pane_widgets: Vec<String>,
}

pub struct LayoutManager {
    pub config: LayoutConfig,
    pane_widget_ids: Vec<WidgetId>,
    dock_state: DockState<Pane>,
    widget_instances: Vec<Box<dyn Widget>>,
    dirty: bool,
}

impl Default for LayoutManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutManager {
    pub fn from_ui_config(ui_config: &UiConfig) -> Self {
        Self::from_persisted(
            &ui_config.visualization_layout_preset,
            &ui_config.visualization_pane_widgets,
        )
    }

    pub fn new() -> Self {
        Self::from_persisted("grid2x2", &[])
    }

    pub fn from_persisted(layout_preset: &str, pane_widgets: &[String]) -> Self {
        let config = LayoutConfig::from_key(layout_preset).unwrap_or(LayoutConfig::Grid2x2);
        let pane_count = config.pane_count();

        let mut pane_widget_ids: Vec<WidgetId> =
            (0..pane_count).map(Self::default_widget_for).collect();
        for (index, key) in pane_widgets.iter().take(pane_count).enumerate() {
            if let Some(widget_id) = widget_from_key(key) {
                pane_widget_ids[index] = widget_id;
            }
        }

        let widget_instances = pane_widget_ids.iter().copied().map(create_widget).collect();

        let dock_state = Self::build_dock_state(config, &pane_widget_ids);

        Self {
            config,
            pane_widget_ids,
            dock_state,
            widget_instances,
            dirty: false,
        }
    }

    fn build_dock_state(config: LayoutConfig, assignments: &[WidgetId]) -> DockState<Pane> {
        let make_tab = |slot: usize| {
            Pane::new(
                slot,
                assignments
                    .get(slot)
                    .copied()
                    .unwrap_or_else(|| Self::default_widget_for(slot)),
            )
        };

        let mut dock_state = DockState::new(vec![make_tab(0)]);
        let surface = dock_state.main_surface_mut();

        match config {
            LayoutConfig::Single => {}
            LayoutConfig::TwoColumns => {
                surface.split_right(NodeIndex::root(), 0.5, vec![make_tab(1)]);
            }
            LayoutConfig::TwoRows => {
                surface.split_below(NodeIndex::root(), 0.5, vec![make_tab(1)]);
            }
            LayoutConfig::Grid2x2 => {
                let [left, right] = surface.split_right(NodeIndex::root(), 0.5, vec![make_tab(1)]);
                surface.split_below(left, 0.5, vec![make_tab(2)]);
                surface.split_below(right, 0.5, vec![make_tab(3)]);
            }
            LayoutConfig::OneLeftTwoRight => {
                let [_left, right] =
                    surface.split_right(NodeIndex::root(), 0.62, vec![make_tab(1)]);
                surface.split_below(right, 0.5, vec![make_tab(2)]);
            }
            LayoutConfig::TwoLeftOneRight => {
                let [left, _right] =
                    surface.split_right(NodeIndex::root(), 0.38, vec![make_tab(2)]);
                surface.split_below(left, 0.5, vec![make_tab(1)]);
            }
        }

        dock_state
    }

    fn default_widget_for(index: usize) -> WidgetId {
        const DEFAULTS: [WidgetId; 4] = [
            WidgetId::TimeSeries,
            WidgetId::FftPlot,
            WidgetId::BandPower,
            WidgetId::SignalQuality,
        ];
        DEFAULTS[index % DEFAULTS.len()]
    }

    fn ensure_assignment_count(&mut self, count: usize) {
        while self.pane_widget_ids.len() < count {
            let idx = self.pane_widget_ids.len();
            self.pane_widget_ids.push(Self::default_widget_for(idx));
            self.widget_instances.push(create_widget(
                *self
                    .pane_widget_ids
                    .last()
                    .unwrap_or(&WidgetId::SignalQuality),
            ));
        }
        self.pane_widget_ids.truncate(count);
        self.widget_instances.truncate(count);
        self.dock_state = Self::build_dock_state(self.config, &self.pane_widget_ids);
    }

    pub fn set_layout(&mut self, config: LayoutConfig) {
        if self.config == config {
            return;
        }

        self.ensure_assignment_count(config.pane_count());
        self.config = config;
        self.dock_state = Self::build_dock_state(self.config, &self.pane_widget_ids);
        self.dirty = true;
    }

    #[allow(dead_code)]
    pub fn set_pane_widget(&mut self, pane_index: usize, widget_id: WidgetId) {
        if let Some(current) = self.pane_widget_ids.get_mut(pane_index)
            && *current != widget_id
        {
            *current = widget_id;
            if let Some(widget) = self.widget_instances.get_mut(pane_index) {
                *widget = create_widget(widget_id);
            }
            self.dock_state = Self::build_dock_state(self.config, &self.pane_widget_ids);
            self.dirty = true;
        }
    }

    pub fn take_persisted_state(&mut self) -> Option<PersistedLayoutState> {
        if !self.dirty {
            return None;
        }

        self.dirty = false;
        Some(PersistedLayoutState {
            layout_preset: self.config.key().to_string(),
            pane_widgets: self
                .pane_widget_ids
                .iter()
                .copied()
                .map(widget_to_key)
                .map(str::to_string)
                .collect(),
        })
    }

    pub fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Layout:");
            let options: Vec<&str> = LayoutConfig::ALL.iter().map(LayoutConfig::label).collect();
            let mut selected_index = LayoutConfig::ALL
                .iter()
                .position(|layout| *layout == self.config)
                .unwrap_or(0);
            if theme::select_index(
                ui,
                "layout_manager_layout_selector",
                &mut selected_index,
                &options,
                160.0,
            ) {
                self.set_layout(LayoutConfig::ALL[selected_index]);
            }
        });
    }

    pub fn show_panes(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>) {
        ui.horizontal_wrapped(|ui| {
            theme::status_chip(ui, "Docking enabled", theme::Intent::Info);
            theme::status_chip(
                ui,
                "Drag tabs to panel edges/centers to re-dock",
                theme::Intent::Muted,
            );
        });

        let mut viewer = DockTabViewer {
            widget_ids: &mut self.pane_widget_ids,
            widget_instances: &mut self.widget_instances,
            widget_ctx: ctx,
            changed: false,
        };
        let mut dock_style = Style::from_egui(ui.style().as_ref());
        dock_style.tab_bar.height = 28.0;
        dock_style.tab.minimum_width = Some(128.0);
        DockArea::new(&mut self.dock_state)
            .id(egui::Id::new("visualization_dock_area"))
            .style(dock_style)
            .draggable_tabs(true)
            .show_tab_name_on_hover(true)
            .show_inside(ui, &mut viewer);

        if viewer.changed {
            self.dirty = true;
        }
    }
}

struct DockTabViewer<'a, 'b> {
    widget_ids: &'a mut [WidgetId],
    widget_instances: &'a mut [Box<dyn Widget>],
    widget_ctx: &'a WidgetContext<'b>,
    changed: bool,
}

impl TabViewer for DockTabViewer<'_, '_> {
    type Tab = Pane;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        format!("#{} {}", tab.slot + 1, tab.widget_id.label()).into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        ui.horizontal(|ui| {
            ui.label("Widget:");
            let options: Vec<&str> = WidgetId::ALL.iter().map(WidgetId::label).collect();
            let mut selected_index = WidgetId::ALL
                .iter()
                .position(|widget_id| *widget_id == tab.widget_id)
                .unwrap_or(0);
            if theme::select_index(
                ui,
                format!("dock_pane_widget_{}", tab.slot),
                &mut selected_index,
                &options,
                180.0,
            ) {
                let selected = WidgetId::ALL[selected_index];
                tab.widget_id = selected;
                if let Some(slot_value) = self.widget_ids.get_mut(tab.slot) {
                    *slot_value = selected;
                }
                if let Some(widget) = self.widget_instances.get_mut(tab.slot) {
                    *widget = create_widget(selected);
                }
                self.changed = true;
            }
        });

        ui.separator();

        if let Some(widget) = self.widget_instances.get_mut(tab.slot) {
            widget.show(ui, self.widget_ctx, tab.slot);
        } else {
            theme::status_chip(ui, "Missing widget instance", theme::Intent::Warning);
        }
    }
}

fn widget_to_key(widget_id: WidgetId) -> &'static str {
    match widget_id {
        WidgetId::TimeSeries => "time_series",
        WidgetId::FftPlot => "fft_plot",
        WidgetId::BandPower => "band_power",
        WidgetId::SignalQuality => "signal_quality",
        WidgetId::DecoderMonitor => "decoder_monitor",
        WidgetId::ActionPreview => "action_preview",
        WidgetId::Accelerometer => "accelerometer",
        WidgetId::Spectrogram => "spectrogram",
        WidgetId::Focus => "focus",
        WidgetId::Headplot => "headplot",
        WidgetId::StreamMetadata => "stream_metadata",
    }
}

fn widget_from_key(value: &str) -> Option<WidgetId> {
    match value {
        "time_series" => Some(WidgetId::TimeSeries),
        "fft_plot" => Some(WidgetId::FftPlot),
        "band_power" => Some(WidgetId::BandPower),
        "signal_quality" => Some(WidgetId::SignalQuality),
        "decoder_monitor" => Some(WidgetId::DecoderMonitor),
        "action_preview" => Some(WidgetId::ActionPreview),
        "accelerometer" => Some(WidgetId::Accelerometer),
        "spectrogram" => Some(WidgetId::Spectrogram),
        "focus" => Some(WidgetId::Focus),
        "headplot" => Some(WidgetId::Headplot),
        "stream_metadata" => Some(WidgetId::StreamMetadata),
        _ => None,
    }
}
