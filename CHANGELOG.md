# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- SDK facade library (`neurohid-sdk`) with feature-gated re-exports
- LSL (Lab Streaming Layer) integration with feature-gating
- Headless service binary (`neurohid-service`)
- CI/CD workflows for testing and publishing
- `service.ipc_simulation_enabled` configuration flag to gate simulated IPC behavior
- Unit tests for signal timing conversion and IPC simulation gating in `neurohid-core`
- Real IPC integration tests for connect/disconnect/reconnect transitions in `neurohid-core` and `neurohid-hub`
- Repository-level governance templates: ADR, planning DoR/DoD, UX checklist, and PR checklist
- Repo-local automation assets for docs freshness, architecture validation, feature planning, TDD enforcement, UX review, and Python ML review
- CI policy workflows: docs freshness gate, architecture ADR gate, Python quality gate, and UV command policy gate
- Structured JSON tracing support for `neurohid` and `neurohid-service` with configurable output via `NEUROHID_LOG_FORMAT`
- Hot-path data-flow tracing across runtime stages with correlation fields (`decision_id`, `stream_id`) and bounded periodic summaries
- Control-plane tracing for service/hub request boundaries (command, request id, response kind, duration)
- Shared observability taxonomy (`stage`/`span`/`event`) and configurable sampling/rate-limit knobs via `service.observability` (global + per-component `signal`, `decoder`, `action`, `ipc`, `control`)
- Hub Python Lab now uses `egui_code_editor` for syntax-highlighted notebook cell editing, and both Hub/Jupyter flows now include `egui-async` task integration for frame-safe background operations
- Jupyter IDE now includes an in-panel command console powered by `egui_console` with built-in commands for bootstrap/start/stop/open/status flows
- Dashboard candidate training/staging jobs now run through `egui-async` bindings instead of manual thread/channel plumbing, improving frame-safe async task handling consistency across Hub screens
- Hub now integrates `egui_logger` with a toggleable in-app Runtime Logs window, and Hub binary startup now uses a combined logger bridge so log events are visible in UI while still flowing through tracing subscribers
- Hub now includes initial `egui_kittest` smoke tests for Python Lab and Jupyter IDE controls to lock in baseline UI behavior for the new async/editor/console flows
- Visualization now includes a rerun pilot bridge (`Connect` + `Mirror`) that streams key runtime metrics (signal quality, emitted actions, buffer samples) to a rerun viewer over SDK logging

### Changed

- Reorganized project into Rust workspace with separate published and internal crates
- NeuroHID Hub UI received a cohesive visual refresh across Dashboard, Visualization, Devices, Profiles, Calibration, Jupyter IDE, and Settings screens, including upgraded dark-theme styling, improved sidebar/status readability, and standardized panel framing without protocol or config schema changes
- Hub default service behavior now auto-starts the core service on app launch via `service.auto_start = true`
- Hub now migrates legacy persisted configs with `service.auto_start = false` to `true` on load so existing installs auto-start the core service on app launch
- Extracted binary crate from library for cleaner architecture
- Published crates: `neurohid` (binary), `neurohid-sdk` (library facade)
- Internal crates: `neurohid-types`, `neurohid-signal`, `neurohid-device`, `neurohid-platform`, `neurohid-storage`, `neurohid-ipc`, `neurohid-calibration`, `neurohid-core`, `neurohid-hub`
- Signal task buffering now uses ring-buffer semantics (`VecDeque`) and per-stream timestamp-based sampling cadence
- Hub sidebar now surfaces explicit IPC mode/status (`Connected`, `Simulated`, `Disconnected`)
- Core IPC task now runs a real TCP bridge to Python when simulation mode is disabled, with automatic reconnect after disconnect
- Core action task placeholder tracking field now uses underscore-prefixed naming to reduce explicit dead-code allowances while preserving future wiring intent
- Workspace Rust baseline updated to edition 2024 and rust-version 1.85
- Python test workflow standardized on `uv` + `pytest` in CI and contributor guidance
- Hub visualization layout engine in `neurohid-hub` now uses `egui_tiles` for pane tiling/resizing/drag-rearrangement while preserving existing layout presets and per-pane widget selection
- Hub now persists visualization pane arrangement, widget assignments, and layout preset across launches via UI config state
- Mixed LSL stream handling now classifies streams by metadata and routes only EEG-like streams into decoder feature extraction, while non-EEG streams remain connected and observable without crashing the service
- Signal feature extraction now gracefully handles low-channel streams (including 1-channel sources) by bounds-checking frontal asymmetry indices instead of panicking

### Removed

- Legacy in-tree Emotiv integration path (replaced by dedicated `emotiv-cortex-v2` and `emotiv-cortex-cli` crates in `https://github.com/jmduea/emotiv-cortex-rs`)
