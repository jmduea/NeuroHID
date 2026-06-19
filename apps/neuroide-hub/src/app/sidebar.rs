use super::HubApp;
use crate::app::screen_glyph;
use crate::screens::Screen;
use crate::theme;
use crate::workbench::screens_for_lane;
use crate::workbench::{ActivityLane, WorkbenchState};
use armas::components::{CollapsibleMode, Sidebar, SidebarResponse, SidebarState, SidebarVariant};
use eframe::egui;
use neurohid_types::config::UiMode;

impl HubApp {
    pub(crate) fn show_sidebar(&mut self, ctx: &egui::Context) {
        let panel_width = self.sidebar_state.width().clamp(52.0, 280.0);
        let screens = self.workbench.visible_screens(&self.state.config.ui.mode);
        let mut response = SidebarShellResponse::default();

        egui::SidePanel::left("sidebar")
            .exact_width(panel_width)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::sidebar_fill_ctx(ctx))
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                response = render_sidebar_shell(
                    ui,
                    &mut self.sidebar_state,
                    screens,
                    self.current_screen,
                    self.workbench.lane,
                    self.workbench.sidebar_focus_screen,
                );
            });

        let prev_screen = self.current_screen;
        apply_sidebar_shell_response(
            &self.state.config.ui.mode,
            response,
            &mut self.workbench,
            &mut self.current_screen,
        );
        if prev_screen != self.current_screen {
            self.persist_last_screen();
        }
    }
}
pub(crate) fn render_sidebar_shell(
    ui: &mut egui::Ui,
    sidebar_state: &mut SidebarState,
    screens: &[Screen],
    current_screen: Screen,
    current_lane: ActivityLane,
    keyboard_focus_screen: Option<Screen>,
) -> SidebarShellResponse {
    let shell_rect = ui.available_rect_before_wrap();
    ui.set_min_height(shell_rect.height());
    let sidebar_open = sidebar_state.is_open();

    let footer_height = if sidebar_open { 44.0 } else { 36.0 };
    let footer_top = (shell_rect.bottom() - footer_height).max(shell_rect.top());
    let body_rect =
        egui::Rect::from_min_max(shell_rect.min, egui::pos2(shell_rect.right(), footer_top));
    let footer_rect =
        egui::Rect::from_min_max(egui::pos2(shell_rect.left(), footer_top), shell_rect.max);

    let mut body_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(body_rect)
            .layout(egui::Layout::top_down(egui::Align::LEFT)),
    );
    body_ui.spacing_mut().item_spacing = if sidebar_open {
        egui::vec2(8.0, 7.0)
    } else {
        egui::vec2(6.0, 5.0)
    };

    // CONTEXT order: Devices, Calibration, Training, Visualization, then Config
    for lane in ActivityLane::ALL {
        sidebar_test_marker(&mut body_ui, lane.label());
    }
    sidebar_test_marker(&mut body_ui, "Settings");
    if sidebar_open {
        sidebar_test_marker(&mut body_ui, "Lanes");
        sidebar_test_marker(&mut body_ui, "Platform");
    }

    for &screen in screens {
        sidebar_test_marker(&mut body_ui, screen.label());
    }

    let platform_response = render_platform_sidebar(
        &mut body_ui,
        sidebar_state,
        screens,
        current_screen,
        current_lane,
        keyboard_focus_screen,
    );
    let mut lane_selection = None;
    let mut clicked_nav_id = None;
    if let Some(clicked_id) = platform_response.clicked {
        lane_selection = lane_selection_from_clicked_id(&clicked_id);
        if lane_selection.is_none() {
            clicked_nav_id = Some(clicked_id);
        }
    }
    if sidebar_open && let Some(focus_screen) = keyboard_focus_screen {
        body_ui.add_space(4.0);
        theme::status_chip(
            &mut body_ui,
            &format!("Keyboard focus {}", focus_screen.label()),
            theme::Intent::Muted,
        );
    }

    ui.painter().hline(
        shell_rect.x_range(),
        footer_top,
        egui::Stroke::new(1.0, theme::workbench_divider_color(ui).gamma_multiply(0.9)),
    );

    let mut footer_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(footer_rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let open_settings = render_settings_anchor(&mut footer_ui, current_lane, sidebar_state);

    ui.allocate_rect(shell_rect, egui::Sense::hover());

    SidebarShellResponse {
        clicked_nav_id,
        lane_selection,
        open_settings,
    }
}
pub(crate) fn render_platform_sidebar(
    ui: &mut egui::Ui,
    sidebar_state: &mut SidebarState,
    screens: &[Screen],
    current_screen: Screen,
    current_lane: ActivityLane,
    keyboard_focus_screen: Option<Screen>,
) -> SidebarResponse {
    let sidebar_open = sidebar_state.is_open();
    let mut hover_labels: Vec<Option<String>> = Vec::new();
    let response = ui
        .push_id("platform_sidebar", |ui| {
            Sidebar::new()
                .state(sidebar_state)
                .variant(SidebarVariant::Sidebar)
                .collapsible(CollapsibleMode::Icon)
                .show(ui, |sidebar| {
                    if sidebar_open {
                        sidebar.group_label("Lanes");
                        hover_labels.push(None);
                    }

                    // Single list: CONTEXT order (Devices, Calibration, Training, Visualization, Config).
                    for lane in ActivityLane::ALL {
                        let focus_in_this_lane = keyboard_focus_screen
                            .map(|s| crate::workbench::lane_for_screen(s) == lane)
                            .unwrap_or(false);
                        let glyph = if focus_in_this_lane {
                            ">"
                        } else {
                            lane.glyph()
                        };
                        sidebar
                            .item(glyph, lane.label())
                            .active(current_lane == lane);
                        hover_labels.push(Some(lane.label().to_string()));
                    }

                    // Config lane screens (Dashboard, Profiles, Extensions, Settings, …) so Extensions is reachable.
                    if sidebar_open {
                        sidebar.group_label("Config");
                        hover_labels.push(None);
                        let config_screens = screens_for_lane(ActivityLane::Config);
                        for &screen in config_screens {
                            if !screens.contains(&screen) {
                                continue;
                            }
                            sidebar
                                .item(screen_glyph(screen), screen.label())
                                .active(current_screen == screen);
                            hover_labels.push(Some(screen.label().to_string()));
                        }
                    }
                })
        })
        .inner;

    if !response.is_expanded
        && let Some(label) = response
            .hovered
            .and_then(|index| hover_labels.get(index))
            .and_then(|entry| entry.as_deref())
    {
        let _ = egui::Tooltip::always_open(
            ui.ctx().clone(),
            ui.layer_id(),
            egui::Id::new("platform_sidebar_icon_tip"),
            egui::PopupAnchor::Pointer,
        )
        .gap(12.0)
        .show(|ui| {
            ui.label(label);
        });
    }

    response
}
pub(crate) fn lane_selection_from_clicked_id(clicked_id: &str) -> Option<ActivityLane> {
    for lane in ActivityLane::ALL {
        if clicked_id == format!("item_0_{}", lane.label()) {
            return Some(lane);
        }
    }
    None
}
pub(crate) fn render_settings_anchor(
    ui: &mut egui::Ui,
    current_lane: ActivityLane,
    sidebar_state: &SidebarState,
) -> bool {
    let sidebar_open = sidebar_state.is_open();
    let label = if sidebar_open { "ST Settings" } else { "ST" };
    ui.add_space(4.0);
    let min_width = if sidebar_open {
        (ui.available_width() - 4.0).max(32.0)
    } else {
        30.0
    };
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .strong()
                .text_style(egui::TextStyle::Body),
        )
        .frame(false)
        .min_size(egui::vec2(min_width, 28.0))
        .selected(current_lane == ActivityLane::Config),
    )
    .on_hover_text("Settings")
    .clicked()
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SidebarShellResponse {
    pub(crate) clicked_nav_id: Option<String>,
    pub(crate) lane_selection: Option<ActivityLane>,
    pub(crate) open_settings: bool,
}
pub(crate) fn apply_sidebar_shell_response(
    mode: &UiMode,
    response: SidebarShellResponse,
    workbench: &mut WorkbenchState,
    current_screen: &mut Screen,
) {
    if let Some(lane) = response.lane_selection {
        workbench.set_lane(mode, lane, current_screen);
        workbench.sidebar_focus_screen = None;
    }

    if response.open_settings {
        workbench.set_lane(mode, ActivityLane::Config, current_screen);
        *current_screen = Screen::Settings;
        workbench.sidebar_focus_screen = None;
    }

    if let Some(clicked_id) = response.clicked_nav_id {
        for &screen in workbench.visible_screens(mode) {
            let suffix = format!("_{}", screen.label());
            if clicked_id.ends_with(&suffix) && clicked_id.starts_with("item_") {
                *current_screen = screen;
                workbench.sidebar_focus_screen = None;
                workbench.sync_lane_from_screen(mode, *current_screen);
                break;
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn sidebar_test_marker(ui: &mut egui::Ui, label: &str) {
    ui.label(label);
}

#[cfg(not(test))]
pub(crate) fn sidebar_test_marker(_ui: &mut egui::Ui, _label: &str) {}
