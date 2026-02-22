use super::HubApp;
use crate::screens::Screen;
use crate::state::ServiceSnapshot;
use crate::theme;
use crate::workbench::{ActivityLane, BottomTab, WorkbenchState};
use eframe::egui;
use neurohid_types::config::UiMode;
use std::path::PathBuf;

impl HubApp {
    pub(crate) fn detached_visualization_viewport_id() -> egui::ViewportId {
        egui::ViewportId::from_hash_of(Self::DETACHED_VISUALIZATION_VIEWPORT_ID_SEED)
    }
    pub(crate) fn set_visualization_detached(&mut self, ctx: &egui::Context, detached: bool) {
        if self.state.config.ui.visualization_detached == detached {
            return;
        }

        self.state.config.ui.visualization_detached = detached;
        if detached {
            self.current_screen = Screen::Visualization;
            self.workbench
                .sync_lane_from_screen(&self.state.config.ui.mode, self.current_screen);
            self.persist_last_screen();
        } else {
            ctx.send_viewport_cmd_to(
                Self::detached_visualization_viewport_id(),
                egui::ViewportCommand::Close,
            );
            self.visualization_detached_native_active = false;
            self.visualization_detached_fallback_warning = false;
        }

        if self.persist_config("detached visualization preference") {
            self.visualization_detached_geometry_dirty = false;
            self.visualization_detached_last_persist_secs = ctx.input(|input| input.time);
        }
    }
    pub(crate) fn toggle_visualization_detached(&mut self, ctx: &egui::Context) {
        self.set_visualization_detached(ctx, !self.state.config.ui.visualization_detached);
    }
    pub(crate) fn detached_visualization_viewport_builder(&self) -> egui::ViewportBuilder {
        let mut builder = egui::ViewportBuilder::default()
            .with_title("NeuroHID Visualization")
            .with_min_inner_size(egui::vec2(
                Self::DETACHED_VISUALIZATION_MIN_WIDTH,
                Self::DETACHED_VISUALIZATION_MIN_HEIGHT,
            ));

        let (width, height) = self.state.config.ui.visualization_detached_size.unwrap_or((
            Self::DETACHED_VISUALIZATION_DEFAULT_WIDTH,
            Self::DETACHED_VISUALIZATION_DEFAULT_HEIGHT,
        ));
        builder = builder.with_inner_size(egui::vec2(
            width.max(Self::DETACHED_VISUALIZATION_MIN_WIDTH),
            height.max(Self::DETACHED_VISUALIZATION_MIN_HEIGHT),
        ));

        if let Some((x, y)) = self.state.config.ui.visualization_detached_pos
            && x.is_finite()
            && y.is_finite()
        {
            builder = builder.with_position(egui::pos2(x, y));
        }

        builder
    }
    pub(crate) fn quantize_window_metric(value: f32) -> Option<f32> {
        if value.is_finite() {
            Some((value * 2.0).round() * 0.5)
        } else {
            None
        }
    }
    pub(crate) fn update_detached_visualization_geometry(
        &mut self,
        pos: egui::Pos2,
        size: egui::Vec2,
    ) {
        let Some(pos_x) = Self::quantize_window_metric(pos.x) else {
            return;
        };
        let Some(pos_y) = Self::quantize_window_metric(pos.y) else {
            return;
        };
        let Some(size_x) = Self::quantize_window_metric(size.x) else {
            return;
        };
        let Some(size_y) = Self::quantize_window_metric(size.y) else {
            return;
        };

        let next_pos = Some((pos_x, pos_y));
        let next_size = Some((
            size_x.max(Self::DETACHED_VISUALIZATION_MIN_WIDTH),
            size_y.max(Self::DETACHED_VISUALIZATION_MIN_HEIGHT),
        ));
        if self.state.config.ui.visualization_detached_pos != next_pos
            || self.state.config.ui.visualization_detached_size != next_size
        {
            self.state.config.ui.visualization_detached_pos = next_pos;
            self.state.config.ui.visualization_detached_size = next_size;
            self.visualization_detached_geometry_dirty = true;
        }
    }
    pub(crate) fn maybe_persist_detached_visualization_geometry(&mut self, ctx: &egui::Context) {
        if !self.state.config.ui.visualization_detached
            || !self.visualization_detached_geometry_dirty
        {
            return;
        }

        let now = ctx.input(|input| input.time);
        if now - self.visualization_detached_last_persist_secs < 1.0 {
            return;
        }

        if self.persist_config("detached visualization geometry") {
            self.visualization_detached_geometry_dirty = false;
            self.visualization_detached_last_persist_secs = now;
        }
    }
    pub(crate) fn show_detached_visualization_viewport(&mut self, ctx: &egui::Context) {
        self.visualization_detached_native_active = false;
        self.visualization_detached_fallback_warning = false;

        if !self.state.config.ui.visualization_detached {
            return;
        }

        let mut close_requested = false;
        let mut geometry_update: Option<(egui::Pos2, egui::Vec2)> = None;
        let mut native_viewport = false;
        let mut fallback_embedded = false;

        ctx.show_viewport_immediate(
            Self::detached_visualization_viewport_id(),
            self.detached_visualization_viewport_builder(),
            |viewport_ctx, class| {
                if class == egui::ViewportClass::Embedded {
                    fallback_embedded = true;
                    return;
                }

                native_viewport = true;
                let (close_now, outer_rect, inner_rect) = viewport_ctx.input(|input| {
                    let viewport = input.viewport();
                    (
                        viewport.close_requested(),
                        viewport.outer_rect,
                        viewport.inner_rect,
                    )
                });
                close_requested |= close_now;
                if let Some(rect) = outer_rect.or(inner_rect) {
                    geometry_update = Some((rect.min, rect.size()));
                }

                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(viewport_ctx.style().visuals.panel_fill)
                            .inner_margin(egui::Margin::symmetric(8, 8)),
                    )
                    .show(viewport_ctx, |ui| {
                        let snapshot = self.state.service_snapshot.clone();
                        self.visualization.show(
                            ui,
                            &self.data_bus,
                            &snapshot,
                            &mut self.state,
                            &self.runtime,
                        );
                    });
            },
        );

        self.visualization_detached_fallback_warning = fallback_embedded;
        self.visualization_detached_native_active = native_viewport && !close_requested;

        if let Some((pos, size)) = geometry_update {
            self.update_detached_visualization_geometry(pos, size);
        }

        if close_requested {
            self.set_visualization_detached(ctx, false);
        }

        self.maybe_persist_detached_visualization_geometry(ctx);
    }
}
