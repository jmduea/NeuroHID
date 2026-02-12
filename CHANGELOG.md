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

### Changed

- Reorganized project into Rust workspace with separate published and internal crates
- Extracted binary crate from library for cleaner architecture
- Published crates: `neurohid` (binary), `neurohid-sdk` (library facade)
- Internal crates: `neurohid-types`, `neurohid-signal`, `neurohid-device`, `neurohid-platform`, `neurohid-storage`, `neurohid-ipc`, `neurohid-calibration`, `neurohid-core`, `neurohid-hub`
- Signal task buffering now uses ring-buffer semantics (`VecDeque`) and per-stream timestamp-based sampling cadence
- Hub sidebar now surfaces explicit IPC mode/status (`Connected`, `Simulated`, `Disconnected`)
- Core IPC task now runs a real TCP bridge to Python when simulation mode is disabled, with automatic reconnect after disconnect
- Core action task placeholder tracking field now uses underscore-prefixed naming to reduce explicit dead-code allowances while preserving future wiring intent

### Removed

- Legacy in-tree Emotiv integration path (replaced by dedicated `emotiv-cortex-v2` and `emotiv-cortex-cli` crates in `https://github.com/jmduea/emotiv-cortex-rs`)
