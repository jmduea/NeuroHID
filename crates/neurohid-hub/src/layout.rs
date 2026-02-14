//! # Layout Engine
//!
//! Manages the multi-pane widget layout for the Visualization screen.
//! Users choose a layout configuration (e.g., 2x2, 1+2), assign
//! a widget to each pane, and can drag/resize panes via `egui_tiles`.

use crate::widgets::{create_widget, Widget, WidgetContext, WidgetId};
use eframe::egui;
use egui_tiles::{Behavior, EditAction, Tile, TileId, Tiles, Tree, UiResponse};

/// Available layout configurations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LayoutConfig {
    /// Single full-width pane.
    Single,
    /// Two panes side by side.
    TwoColumns,
    /// Two panes stacked vertically.
    TwoRows,
    /// 2x2 grid.
    Grid2x2,
    /// One large pane on the left, two smaller on the right.
    OneLeftTwoRight,
    /// Two smaller on the left, one large on the right.
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

    /// Number of panes in this layout.
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
    pub tree_json: Option<String>,
}

/// The layout manager for the Visualization workspace.
pub struct LayoutManager {
    pub config: LayoutConfig,
    pane_widget_ids: Vec<WidgetId>,
    tree: Tree<Pane>,
    widget_instances: Vec<Box<dyn Widget>>,
    dirty: bool,
}

impl Default for LayoutManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutManager {
    /// Create a new layout manager with default widget assignments.
    pub fn new() -> Self {
        Self::from_persisted("grid2x2", &[], None)
    }

    /// Restore layout manager from persisted keys and serialized tree state.
    pub fn from_persisted(
        layout_preset: &str,
        pane_widgets: &[String],
        tree_json: Option<&str>,
    ) -> Self {
        let config = LayoutConfig::from_key(layout_preset).unwrap_or(LayoutConfig::Grid2x2);
        let pane_count = config.pane_count();

        let mut pane_widget_ids: Vec<WidgetId> = (0..pane_count).map(Self::default_widget_for).collect();
        for (index, key) in pane_widgets.iter().take(pane_count).enumerate() {
            if let Some(widget_id) = widget_from_key(key) {
                pane_widget_ids[index] = widget_id;
            }
        }

        let mut tree = Self::build_tree(config, &pane_widget_ids);
        if let Some(raw) = tree_json
            && let Ok(parsed_tree) = serde_json::from_str::<Tree<Pane>>(raw)
                && let Some(from_tree) = Self::assignments_from_tree(&parsed_tree, pane_count) {
                    pane_widget_ids = from_tree;
                    tree = parsed_tree;
                }

        let widget_instances = pane_widget_ids
            .iter()
            .copied()
            .map(create_widget)
            .collect();

        Self {
            config,
            pane_widget_ids,
            tree,
            widget_instances,
            dirty: false,
        }
    }

    fn assignments_from_tree(tree: &Tree<Pane>, pane_count: usize) -> Option<Vec<WidgetId>> {
        let mut assignments = vec![None; pane_count];
        let mut pane_tiles = 0usize;

        for (_, tile) in tree.tiles.iter() {
            if let Tile::Pane(pane) = tile {
                pane_tiles = pane_tiles.saturating_add(1);
                if pane.slot >= pane_count {
                    return None;
                }
                if assignments[pane.slot].is_some() {
                    return None;
                }
                assignments[pane.slot] = Some(pane.widget_id);
            }
        }

        if pane_tiles != pane_count {
            return None;
        }

        assignments.into_iter().collect()
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
            self.widget_instances
                .push(create_widget(*self.pane_widget_ids.last().unwrap_or(&WidgetId::SignalQuality)));
        }
        self.pane_widget_ids.truncate(count);
        self.widget_instances.truncate(count);
    }

    fn build_tree(config: LayoutConfig, assignments: &[WidgetId]) -> Tree<Pane> {
        let mut tiles = Tiles::default();
        let pane_ids: Vec<TileId> = assignments
            .iter()
            .copied()
            .enumerate()
            .map(|(slot, id)| tiles.insert_pane(Pane::new(slot, id)))
            .collect();

        let root = match config {
            LayoutConfig::Single => pane_ids[0],
            LayoutConfig::TwoColumns => tiles.insert_horizontal_tile(vec![pane_ids[0], pane_ids[1]]),
            LayoutConfig::TwoRows => tiles.insert_vertical_tile(vec![pane_ids[0], pane_ids[1]]),
            LayoutConfig::Grid2x2 => {
                let top_row = tiles.insert_horizontal_tile(vec![pane_ids[0], pane_ids[1]]);
                let bottom_row = tiles.insert_horizontal_tile(vec![pane_ids[2], pane_ids[3]]);
                tiles.insert_vertical_tile(vec![top_row, bottom_row])
            }
            LayoutConfig::OneLeftTwoRight => {
                let right = tiles.insert_vertical_tile(vec![pane_ids[1], pane_ids[2]]);
                tiles.insert_horizontal_tile(vec![pane_ids[0], right])
            }
            LayoutConfig::TwoLeftOneRight => {
                let left = tiles.insert_vertical_tile(vec![pane_ids[0], pane_ids[1]]);
                tiles.insert_horizontal_tile(vec![left, pane_ids[2]])
            }
        };

        Tree::new("visualization_layout_tree", root, tiles)
    }

    /// Change the layout configuration, keeping as many pane assignments
    /// as possible and filling new panes with defaults.
    pub fn set_layout(&mut self, config: LayoutConfig) {
        if self.config == config {
            return;
        }

        self.ensure_assignment_count(config.pane_count());
        self.config = config;
        self.tree = Self::build_tree(self.config, &self.pane_widget_ids);
        self.dirty = true;
    }

    /// Change the widget in a specific pane.
    #[allow(dead_code)]
    pub fn set_pane_widget(&mut self, pane_index: usize, widget_id: WidgetId) {
        if let Some(current) = self.pane_widget_ids.get_mut(pane_index)
            && *current != widget_id {
                *current = widget_id;
                if let Some(widget) = self.widget_instances.get_mut(pane_index) {
                    *widget = create_widget(widget_id);
                }
                self.tree = Self::build_tree(self.config, &self.pane_widget_ids);
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
            tree_json: serde_json::to_string(&self.tree).ok(),
        })
    }

    /// Render the layout toolbar (layout selector).
    pub fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Layout:");
            let mut selected_layout = self.config;
            let current_label = selected_layout.label();
            egui::ComboBox::from_id_salt("layout_selector")
                .selected_text(current_label)
                .show_ui(ui, |ui: &mut egui::Ui| {
                    for &layout in LayoutConfig::ALL {
                        if ui
                            .selectable_value(&mut selected_layout, layout, layout.label())
                            .changed()
                        {
                            self.set_layout(selected_layout);
                        }
                    }
                });
        });
    }

    /// Render all panes with their widgets.
    pub fn show_panes(&mut self, ui: &mut egui::Ui, ctx: &WidgetContext<'_>) {
        let mut behavior = LayoutBehavior {
            widget_ids: &mut self.pane_widget_ids,
            widget_instances: &mut self.widget_instances,
            widget_ctx: ctx,
            changed: false,
        };
        self.tree.ui(&mut behavior, ui);
        if behavior.changed {
            self.dirty = true;
        }
    }
}

struct LayoutBehavior<'a, 'b> {
    widget_ids: &'a mut [WidgetId],
    widget_instances: &'a mut [Box<dyn Widget>],
    widget_ctx: &'a WidgetContext<'b>,
    changed: bool,
}

impl Behavior<Pane> for LayoutBehavior<'_, '_> {
    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        format!("#{} {}", pane.slot + 1, pane.widget_id.label()).into()
    }

    fn pane_ui(&mut self, ui: &mut egui::Ui, tile_id: TileId, pane: &mut Pane) -> UiResponse {
        ui.horizontal(|ui| {
            ui.label("Widget:");
            let mut selected = pane.widget_id;
            egui::ComboBox::from_id_salt(("pane_widget", tile_id))
                .selected_text(selected.label())
                .show_ui(ui, |ui| {
                    for &wid in WidgetId::ALL {
                        ui.selectable_value(&mut selected, wid, wid.label());
                    }
                });

            if selected != pane.widget_id {
                pane.widget_id = selected;
                if let Some(slot_value) = self.widget_ids.get_mut(pane.slot) {
                    *slot_value = selected;
                }
                if let Some(widget) = self.widget_instances.get_mut(pane.slot) {
                    *widget = create_widget(selected);
                }
                self.changed = true;
            }
        });

        ui.separator();

        if ui.available_height() < 40.0 {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Panel too small").small());
            });
            return UiResponse::None;
        }

        let available = ui.available_size();
        let width_scale = (available.x / 700.0).clamp(0.72, 1.0);
        let height_scale = (available.y / 360.0).clamp(0.72, 1.0);
        let fit_scale = width_scale.min(height_scale);

        ui.scope(|ui| {
            let mut style: egui::Style = ui.style().as_ref().clone();
            style.spacing.item_spacing *= fit_scale;
            style.spacing.button_padding *= fit_scale;

            for font_id in style.text_styles.values_mut() {
                font_id.size = (font_id.size * fit_scale).max(10.0);
            }

            ui.set_style(style);

            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(6, 6))
                .show(ui, |ui| {
                    if let Some(widget) = self.widget_instances.get_mut(pane.slot) {
                        widget.show(ui, self.widget_ctx, pane.slot);
                    } else {
                        ui.colored_label(egui::Color32::YELLOW, "Missing widget instance");
                    }
                });
        });

        UiResponse::None
    }

    fn on_edit(&mut self, _edit_action: EditAction) {
        self.changed = true;
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        0.0
    }

    fn min_size(&self) -> f32 {
        240.0
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: false,
            ..Default::default()
        }
    }

    fn retain_pane(&mut self, _pane: &Pane) -> bool {
        true
    }

    fn tab_title_for_tile(&mut self, tiles: &Tiles<Pane>, tile_id: TileId) -> egui::WidgetText {
        if let Some(Tile::Pane(pane)) = tiles.get(tile_id) {
            return self.tab_title_for_pane(pane);
        }
        "Pane".into()
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
