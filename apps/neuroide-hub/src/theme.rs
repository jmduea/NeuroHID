use armas::components::{
    CardVariant, Input, Progress, Select, SelectOption, Slider, Textarea, Toggle, ToggleSize,
    ToggleVariant,
};
use armas::prelude::{
    ArmasContextExt, Button, ButtonSize, ButtonVariant, Card, Theme as ArmasTheme,
};
use eframe::egui::{self, Color32, RichText};
use neurohid_types::config::ThemeMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intent {
    Info,
    Success,
    Warning,
    Danger,
    Muted,
}

#[derive(Clone, Copy)]
pub enum ButtonTone {
    Primary,
    Secondary,
    Ghost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationState {
    Idle,
    Running,
    Ready,
    Failed,
}

pub const WORKBENCH_STATUS_BAR_HEIGHT: f32 = 30.0;
pub const WORKBENCH_BOTTOM_MIN_HEIGHT: f32 = 150.0;
pub const WORKBENCH_BOTTOM_MAX_HEIGHT: f32 = 460.0;
pub const WORKBENCH_STATUS_DIVIDER_GAP: f32 = 12.0;
pub const WORKBENCH_STATUS_ITEM_HEIGHT: f32 = 22.0;

pub struct ArmasFrame {
    variant: CardVariant,
    margin: egui::Margin,
    fill: Option<Color32>,
    stroke: Option<Color32>,
}

impl ArmasFrame {
    pub fn variant(mut self, variant: CardVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn margin(mut self, margin: egui::Margin) -> Self {
        self.margin = margin;
        self
    }

    pub fn fill(mut self, fill: Color32) -> Self {
        self.fill = Some(fill);
        self
    }

    pub fn stroke(mut self, stroke: Color32) -> Self {
        self.stroke = Some(stroke);
        self
    }

    pub fn show<R>(
        self,
        ui: &mut egui::Ui,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let theme = ui.ctx().armas_theme();
        let mut card = Card::new().variant(self.variant).margin(self.margin);

        if let Some(fill) = self.fill {
            card = card.fill(fill);
        }

        if let Some(stroke) = self.stroke {
            card = card.stroke(stroke);
        }

        let response = card.show(ui, &theme, add_contents);
        egui::InnerResponse {
            inner: response.inner,
            response: response.response,
        }
    }
}

pub fn armas_enabled() -> bool {
    true
}

pub fn apply_ui_preferences(ctx: &egui::Context, theme_mode: ThemeMode, font_scale: f32) {
    ctx.set_pixels_per_point(font_scale.clamp(0.75, 2.0));

    match theme_mode {
        ThemeMode::Light => {
            ctx.set_visuals(egui::Visuals::light());
            ctx.set_armas_theme(ArmasTheme::light());
        }
        ThemeMode::Dark | ThemeMode::System => {
            let mut visuals = egui::Visuals::dark();
            visuals.window_fill = Color32::from_rgb(9, 10, 14);
            visuals.panel_fill = Color32::from_rgb(8, 10, 14);
            visuals.extreme_bg_color = Color32::from_rgb(5, 7, 10);
            visuals.faint_bg_color = Color32::from_rgb(14, 18, 24);

            visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(17, 22, 30);
            visuals.widgets.noninteractive.bg_stroke =
                egui::Stroke::new(1.0, Color32::from_rgb(36, 46, 61));

            visuals.widgets.inactive.bg_fill = Color32::from_rgb(21, 27, 37);
            visuals.widgets.inactive.bg_stroke =
                egui::Stroke::new(1.0, Color32::from_rgb(43, 57, 74));

            visuals.widgets.hovered.bg_fill = Color32::from_rgb(30, 39, 53);
            visuals.widgets.hovered.bg_stroke =
                egui::Stroke::new(1.0, Color32::from_rgb(92, 145, 231));

            visuals.widgets.active.bg_fill = Color32::from_rgb(35, 45, 62);
            visuals.widgets.active.bg_stroke =
                egui::Stroke::new(1.0, Color32::from_rgb(122, 178, 255));

            visuals.selection.bg_fill = Color32::from_rgb(38, 65, 108);
            visuals.selection.stroke = egui::Stroke::new(1.0, Color32::from_rgb(129, 183, 255));

            visuals.window_corner_radius = egui::CornerRadius::same(10);
            ctx.set_visuals(visuals);

            ctx.set_armas_theme(ArmasTheme::dark());
        }
    }

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 7.0);
    style.spacing.button_padding = egui::vec2(11.0, 6.0);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(23.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Name("Section".into()),
        egui::FontId::new(16.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(11.5, egui::FontFamily::Proportional),
    );
    ctx.set_style(style);
}

pub fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(
        RichText::new(title)
            .text_style(egui::TextStyle::Heading)
            .strong(),
    );
    ui.label(RichText::new(subtitle).small().weak());
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(8.0);
}

pub fn status_chip(ui: &mut egui::Ui, label: &str, intent: Intent) {
    let fg = intent_color(intent);
    let bg = fg.gamma_multiply(0.16);
    let stroke = fg.gamma_multiply(0.45);
    egui::Frame::new()
        .corner_radius(egui::CornerRadius::same(6))
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, stroke))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(label).small().color(fg).strong());
        });
}

pub fn operation_state_chip(ui: &mut egui::Ui, subject: &str, state: OperationState) {
    let (suffix, intent) = match state {
        OperationState::Idle => ("idle", Intent::Muted),
        OperationState::Running => ("running", Intent::Warning),
        OperationState::Ready => ("ready", Intent::Success),
        OperationState::Failed => ("failed", Intent::Danger),
    };
    status_chip(ui, &format!("{subject} {suffix}"), intent);
}

pub fn workbench_surface_fill_ctx(ctx: &egui::Context) -> Color32 {
    ctx.style().visuals.window_fill.gamma_multiply(0.97)
}

pub fn workbench_divider_color(ui: &egui::Ui) -> Color32 {
    ui.style().visuals.widgets.noninteractive.bg_stroke.color
}

pub fn card_frame(_ui: &egui::Ui) -> ArmasFrame {
    ArmasFrame {
        variant: CardVariant::Outlined,
        margin: egui::Margin::symmetric(10, 8),
        fill: None,
        stroke: None,
    }
}

pub fn panel_frame(ui: &egui::Ui) -> ArmasFrame {
    ArmasFrame {
        variant: CardVariant::Outlined,
        margin: egui::Margin::symmetric(8, 6),
        fill: Some(ui.style().visuals.window_fill.gamma_multiply(0.94)),
        stroke: Some(ui.style().visuals.widgets.noninteractive.bg_stroke.color),
    }
}

pub fn intent_color(intent: Intent) -> Color32 {
    match intent {
        Intent::Info => Color32::from_rgb(101, 168, 255),
        Intent::Success => Color32::from_rgb(106, 227, 130),
        Intent::Warning => Color32::from_rgb(255, 204, 90),
        Intent::Danger => Color32::from_rgb(255, 107, 107),
        Intent::Muted => Color32::from_rgb(136, 150, 168),
    }
}

pub fn action_button(ui: &mut egui::Ui, label: &str, enabled: bool, tone: ButtonTone) -> bool {
    if enabled {
        let variant = match tone {
            ButtonTone::Primary => ButtonVariant::Default,
            ButtonTone::Secondary => ButtonVariant::Secondary,
            ButtonTone::Ghost => ButtonVariant::Ghost,
        };
        let theme = ui.ctx().armas_theme();
        let response = Button::new(label)
            .variant(variant)
            .size(ButtonSize::Small)
            .show(ui, &theme);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        });
        return response.clicked();
    }

    ui.add_enabled(false, egui::Button::new(label)).clicked()
}

pub fn nav_button(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let theme = ui.ctx().armas_theme();
    let variant = if selected {
        ButtonVariant::Secondary
    } else {
        ButtonVariant::Ghost
    };
    let response = Button::new(label)
        .variant(variant)
        .size(ButtonSize::Small)
        .full_width(true)
        .show(ui, &theme);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    response
}

pub fn text_input(
    ui: &mut egui::Ui,
    id: impl Into<egui::Id>,
    value: &mut String,
    placeholder: &str,
    width: f32,
) -> bool {
    let theme = ui.ctx().armas_theme();
    Input::new(placeholder)
        .id(id)
        .width(width)
        .show(ui, value, &theme)
        .changed
}

pub fn textarea_input(
    ui: &mut egui::Ui,
    id: impl Into<egui::Id>,
    value: &mut String,
    placeholder: &str,
    rows: usize,
    width: f32,
) -> bool {
    let _ = width;
    Textarea::new(placeholder)
        .id(id)
        .rows(rows)
        .show(ui, value)
        .changed
}

pub fn textarea_readonly(
    ui: &mut egui::Ui,
    id: impl Into<egui::Id>,
    value: &mut String,
    rows: usize,
    width: f32,
) -> bool {
    ui.add(
        egui::TextEdit::multiline(value)
            .id(id.into())
            .font(egui::TextStyle::Monospace)
            .desired_rows(rows)
            .desired_width(width)
            .interactive(false),
    )
    .changed()
}

pub fn toggle_switch(ui: &mut egui::Ui, id: impl Into<egui::Id>, value: &mut bool) -> bool {
    let theme = ui.ctx().armas_theme();
    Toggle::new()
        .id(id)
        .variant(ToggleVariant::Switch)
        .size(ToggleSize::Small)
        .show(ui, value, &theme)
        .changed
}

pub fn slider_f32(
    ui: &mut egui::Ui,
    id: impl Into<egui::Id>,
    value: &mut f32,
    min: f32,
    max: f32,
    suffix: Option<&str>,
) -> bool {
    let theme = ui.ctx().armas_theme();
    let mut slider = Slider::new(min, max).id(id).show_value(true).width(180.0);
    if let Some(suffix) = suffix {
        slider = slider.suffix(suffix);
    }
    slider.show(ui, value, &theme).changed
}

pub fn drag_value<T>(
    ui: &mut egui::Ui,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    speed: f64,
    suffix: Option<&str>,
) -> bool
where
    T: egui::emath::Numeric,
{
    let mut drag = egui::DragValue::new(value).range(range).speed(speed);
    if let Some(suffix) = suffix {
        drag = drag.suffix(suffix);
    }
    ui.add(drag).changed()
}

pub fn select_index(
    ui: &mut egui::Ui,
    id: impl Into<egui::Id>,
    selected_index: &mut usize,
    options: &[&str],
    width: f32,
) -> bool {
    if options.is_empty() {
        return false;
    }

    if *selected_index >= options.len() {
        *selected_index = 0;
    }

    let theme = ui.ctx().armas_theme();
    let select_options = options
        .iter()
        .enumerate()
        .map(|(idx, label)| SelectOption::new(idx.to_string(), (*label).to_string()))
        .collect();

    let mut select = Select::new(select_options)
        .id(id)
        .selected(selected_index.to_string())
        .searchable(false)
        .width(width);

    let response = select.show(ui, &theme);
    if response.changed
        && let Some(new_value) = response.selected_value
        && let Ok(new_index) = new_value.parse::<usize>()
        && new_index < options.len()
    {
        *selected_index = new_index;
        return true;
    }

    false
}

pub fn progress_bar(ui: &mut egui::Ui, value_0_to_1: f32, width: f32) -> egui::Response {
    let theme = ui.ctx().armas_theme();
    Progress::new((value_0_to_1.clamp(0.0, 1.0)) * 100.0)
        .width(width)
        .show(ui, &theme)
}

#[cfg(test)]
mod tests {
    use egui_kittest::{Harness, kittest::Queryable};

    use super::{ButtonTone, action_button, nav_button};

    #[test]
    fn armas_button_wrappers_expose_accessible_labels() {
        let harness = Harness::new_ui_state(
            |ui, _state: &mut ()| {
                let _ = action_button(ui, "Wrapper Action", true, ButtonTone::Primary);
                let _ = nav_button(ui, "Wrapper Nav", false);
            },
            (),
        );

        harness.get_by_label("Wrapper Action");
        harness.get_by_label("Wrapper Nav");
    }
}
