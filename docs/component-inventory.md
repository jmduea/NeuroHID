# Component Inventory

## Rust Runtime Components

- `neurohid-core`: orchestrates runtime loops and service behavior
- `neurohid-device`: device integration backends and stream ingestion
- `neurohid-signal`: preprocessing + feature extraction components
- `neurohid-platform`: HID output abstraction layer
- `neurohid-ipc`: bridge transport and protocol mapping
- `neurohid-storage`: secure persistence and profile/model data handling

## Hub UI Components (egui)

Notable screen-level components in `neurohid-hub`:

- Dashboard
- Devices
- Profiles
- Calibration
- Visualization
- Python Lab / Jupyter IDE
- Settings

Supporting UI/widget patterns include dockable panes, status chips, controls, and graph-like data
widgets for runtime signal/decoder feedback.

## Python ML Components

- Bridge client
- Decoder module
- ErrP classifier module
- Trainer module
- CLI and notebook integration support

## Shared Contract Component

- `neurohid-types`: canonical shared type layer used to align Rust runtime and integration clients.
