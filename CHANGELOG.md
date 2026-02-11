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

### Changed

- Reorganized project into Rust workspace with separate published and internal crates
- Extracted binary crate from library for cleaner architecture
- Published crates: `neurohid` (binary), `neurohid-sdk` (library facade)
- Internal crates: `neurohid-types`, `neurohid-signal`, `neurohid-device`, `neurohid-platform`, `neurohid-storage`, `neurohid-ipc`, `neurohid-calibration`, `neurohid-core`, `neurohid-hub`

### Removed

- Emotiv device support (extracted to separate repository)
