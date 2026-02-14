# Visualization Docking Guide

Current state: Hub Visualization uses `egui_dock` as the standard, always-on docking backend.

## Interaction Notes

- Dock mode seeds a multi-panel layout from the selected preset (`Single`, `2x2`, `1+2`, etc.) so docking targets exist immediately.
- Drag operations use the tab title bar (for example, `#1 Time Series`) rather than the chart body.
- Drop on panel edge indicators to split or panel center to merge.

## UI Consistency Rules

- Keep global visual tokens and control wrappers in `neurohid-hub/src/theme.rs`.
- Prefer `theme::action_button` and `theme::nav_button` for command controls.
- Wrap docking/plot surfaces in shared panel/frame helpers for consistent styling.

## Scope Boundaries

- Do not re-introduce deprecated backend toggles or migration-only config flags.
- Keep layout persistence focused on `visualization_layout_preset` and `visualization_pane_widgets`.

## Armas Migration Coverage

The Hub now uses an always-on Armas-first component layer for interactive controls and shell
composition, while keeping `egui_dock` for pane docking behavior.

### Shell and Surface Baseline

- `app.rs` uses Armas `Sidebar` as the standard left navigation shell.
- `theme.rs` is the single-source wrapper layer for shared control and surface composition.
- `card_frame` and `panel_frame` render through Armas `Card` for consistent container styling.

### Control Wrapper Matrix (`theme.rs`)

- `text_input` → Armas `Input`
- `textarea_input` → Armas `Textarea`
- `toggle_switch` → Armas `Toggle`
- `slider_f32` → Armas `Slider`
- `select_index` → Armas `Select`
- `progress_bar` → Armas `Progress`
- `drag_value` → shared numeric drag wrapper for integer/float ranges

### Screen/Widget Adoption

- Settings, Dashboard, Devices, Visualization, Python Lab, Jupyter IDE, Stream Console, and
  visualization widgets now route primary interactive controls through shared Armas wrappers.
- Numeric controls in Hub screens/widgets now route through `theme::drag_value` instead of direct
  per-screen `egui::DragValue` usage.
- Keep direct egui usage only for low-level docking/painting internals where Armas wrappers are
  not applicable.
