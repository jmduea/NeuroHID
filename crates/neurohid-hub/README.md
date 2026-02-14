# neurohid-hub

NeuroHID Hub GUI library.

## Features

- Unified egui application for device management
- Interactive calibration game launcher
- Profile management and configuration editing
- Service start/stop control with live status monitoring
- Visualization workspace with draggable/resizable docked panes (`egui_dock`)
- Persisted visualization workspace state (layout preset and pane widgets)
- Always-on `armas` component layer for shell navigation and action controls
- Armas `Sidebar` shell navigation (`app.rs`) with floating/icon-collapsible behavior
- Single-source theming via `src/theme.rs` (global visuals/tokens + shared control wrappers)
- `card_frame` and `panel_frame` wrappers backed by Armas `Card` for consistent surfaces
- Shared Armas-backed control wrappers for text input, text areas, toggles, sliders, selects, and progress display
- Settings/Dashboard/Devices/Visualization/Python Lab/Jupyter IDE/Stream Console controls migrated to wrapper-backed Armas components
- `egui_dock` is the standard visualization layout engine (no feature gate)

## Usage

This crate is a library for the `neurohid` binary. End users should use the `neurohid-sdk` facade crate with the `hub` feature enabled.

```toml
[dependencies]
neurohid-sdk = { version = "0.1", features = ["hub"] }
```

To run the GUI:

```bash
cargo run -p neurohid
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
