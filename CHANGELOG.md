# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- In-process Python bindings (`neurohid-py` crate) via PyO3 0.28 targeting free-threaded Python 3.14+ — replaces socket-based IPC shim with direct `RuntimeHandle` access, async stream iterators, and trainer protocol methods (see [ADR-001](docs/adr/ADR-001-in-process-python-bindings.md))
- Repository root dual-license texts (`LICENSE-MIT`, `LICENSE-APACHE`) to match declared workspace licensing policy
- Device discovery→connection lifecycle design reference at `docs/plans/2026-02-15-device-discovery-connection-design.md`, including interactive/headless flow mapping and troubleshooting guidance
- BMAD-native NeuroHID automation module scaffold at `_bmad/neurohid/*` with registered workflows `neurohid-phase-workflow` and `migrate-legacy-infra`, plus top-level guidance migration in `AGENTS.md`
- Canonical automation backbone: impact classifier (`.github/scripts/classify-impact.ps1`), local/CI quality runner (`.github/scripts/run-agent-ready-tasks.ps1`), and policy validators for docs freshness, unsafe compliance, and protocol contracts
- Coverage quality gates in CI for both Rust (`cargo llvm-cov`) and Python (`pytest-cov`) with enforced minimum line-coverage thresholds and uploaded coverage artifacts
- Coverage reporting integration via Codecov uploads (Rust `lcov.info`, Python `coverage.xml`) and top-level README coverage badge
- Branch policy enforcement workflow (`.github/workflows/branch-policy.yml`) with inline GitHub API validation to require PR-based updates to `main`
- Architecture index automation via `.github/scripts/generate-architecture-index.ps1` with tracked output at `docs/architecture/index.md`
- CI enhancements for impact-aware job routing, focused gate execution, unsafe compliance, protocol contract validation, and harness smoke report artifact publishing
- Executable protocol-documentation contract test in `neurohid-types` (IPC protocol tests)
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
- Visualization migration guidance with phased `armas` and constrained `egui_dock` adoption
- Default multi-agent phase workflow contract at `_bmad/neurohid/workflows/neurohid-phase-workflow/workflow.md` with routing precedence and completion-phase artifacts
- Agent routing integrity workflow `.github/workflows/agent-routing-integrity.yml` with hook schema checks, route integrity checks, and fixture-based regression checks
- Hook policy validators: `.github/hooks/validate-routing.ps1`, `.github/hooks/test-validate-routing.ps1`, and `.github/hooks/validate-doc-contracts.ps1`

### Changed

- **IPC type simplification**: removed `ControlRpcRequest`, `ControlRpcResponse`, `ControlRpcResponsePayload` wrapper types from `neurohid-ipc` — all IPC control paths now use `ControlRequest`/`ControlResponse` from `neurohid-types` directly (wire format unchanged)
- **IPC type simplification**: merged `RuntimeMlKind` enum into `TrainerStreamKind` (identical variants, single canonical type)
- **Config cleanup**: consolidated 19 serde default functions into struct-level `#[serde(default)]` + `Default` impls; deleted dead `ServiceState` from `neurohid-types::config`; deleted unused XDF parsing types from `neurohid-types::signal`
- **Runtime cleanup**: centralized `ServiceState` → `ControlSnapshot` projection into `to_control_snapshot()` method; extracted `ack_command()` helper to deduplicate 11 identical dispatch match arms
- **Signal pipeline**: cached Welch PSD from feature extraction for temporal state updates — eliminates redundant Goertzel band-power approximation per extraction cycle
- Python ML bridge (`neurohid-ml`) migrated from socket-based IPC to in-process PyO3 bindings: `IpcClient` now wraps `RuntimeHandle.trainer_*()`, control client takes `RuntimeHandle` directly, telemetry client wraps `subscribe_events()`, CLI commands use `RuntimeBuilder` instead of IPC endpoint arguments
- Python package `neurohid_bindings` renamed to `neurohid` (module name matches `#[pymodule]`)
- Python version requirement bumped to `>=3.14` (free-threaded CPython)
- Private-phase CI/workflow runner policy now targets dedicated self-hosted labels (`self-hosted` + OS + `neurohid-ci`) across branch policy, CI, architecture, crate-boundaries, Python quality, release, and publish workflows, with a `ci.yml` macOS lane toggle (`ENABLE_MACOS`) to allow pragmatic macOS de-scope when needed
- Pre-merge validation and coverage enforcement behavior is unchanged under self-hosted execution (Rust/Python quality gates plus `PYTHON_COVERAGE_MIN` and `RUST_COVERAGE_MIN` thresholds remain active)
- Workspace `lsl-sys` patch source now uses a shared git-pinned upstream (`[patch.crates-io]` with fixed `rev`) for reproducible Linux behavior across multiple applications without repo-local vendoring
- Reorganized project into Rust workspace with separate published and internal crates
- Coverage policy now enforces a 90% Codecov patch-coverage target for pull requests, gating newly added/changed code independently from overall project coverage baseline
- Release automation now separates tag-based pre-release verification (`.github/workflows/release.yml`) from manual crates.io publishing (`.github/workflows/publish-crates.yml`)
- Deferred in-app rerun integration for now; keep as a potential future optional visualization backend/replacement once runtime footprint and UX tradeoffs are re-evaluated
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
- Hub visualization layout engine in `neurohid-hub` now uses `egui_dock` as the standard pane docking/rearrangement system while preserving existing layout presets and per-pane widget selection
- Hub now persists visualization pane arrangement, widget assignments, and layout preset across launches via UI config state
- Mixed LSL stream handling now classifies streams by metadata and routes only EEG-like streams into decoder feature extraction, while non-EEG streams remain connected and observable without crashing the service
- Signal feature extraction now gracefully handles low-channel streams (including 1-channel sources) by bounds-checking frontal asymmetry indices instead of panicking
- Hub UI now uses an always-on Armas-first component layer (no runtime pilot gate), with centralized theme/style primitives in `neurohid-hub/src/theme.rs` applied across shell, screens, and primary action controls
- Hub shell navigation now uses `armas::components::Sidebar` (floating, icon-collapsible) in `neurohid-hub/src/app.rs`, replacing the prior custom sidebar composition
- Theme wrappers `card_frame` and `panel_frame` in `neurohid-hub/src/theme.rs` now render through `armas::components::Card` (`CardVariant::Outlined`) so screen containers inherit a single Armas-backed surface implementation
- Stream Console control actions (close/clear/pause/filter-clear) now use shared Armas-backed button wrappers instead of raw `egui::Button` instances
- Hub screen controls across Settings, Dashboard, Devices, Visualization, Python Lab, and Jupyter IDE now route through shared Armas wrappers for select/input/toggle/slider/textarea/progress interactions
- Visualization widget toolbars (`fft_plot`, `band_power`, `time_series`, `action_preview`, `accelerometer`, `focus`, `headplot`, `spectrogram`) now use shared Armas-backed navigation/control wrappers for interaction consistency
- Progress indicators in Dashboard/Devices/widgets now render through shared Armas progress wrapper primitives
- Visualization now uses always-on `egui_dock` with no backend feature gating; legacy `visualization_docking_backend` config selection has been retired while layout preset/widget persistence remains intact
- Python Lab screen is re-enabled in Advanced mode sidebar routing and active central-panel dispatch
- Hub numeric controls now route through shared `theme::drag_value` wrappers (replacing direct per-screen `egui::DragValue` usage in Settings, Stream Console, and visualization widgets)
- Calibration crate interaction controls now use Armas button/progress primitives for consistent component usage across Hub and embedded calibration flows
- Hub shell status bar now keeps Console/Logs toggles always available (running or stopped) with consistent button tones, and sidebar footer version styling now follows shared weak-text semantics
- Comprehensive Hub UI status-chip migration: all screens (Dashboard, Devices, Profiles, Calibration, Visualization, Python Lab, Jupyter IDE, Settings, Stream Console) and visualization widgets now use shared Armas-backed status chips instead of color-only indicators, improving readability and visual consistency across the entire UI
- Hub visualization repaint scheduling now uses throttled `request_repaint_after` cadence (instead of continuous immediate repaint), reducing idle/high-load UI CPU usage while preserving live responsiveness
- Settings now includes `Visualization FPS` (persisted in `ui.visualization_target_fps`, default 30, range 5-60) so users can tune smoothness vs CPU usage
- Calibration wizard now includes explicit step count and progress bar for clearer multi-step orientation during setup
- Repo automation routing was consolidated to existing agent inventory with writer-owned documentation freshness, completion-finisher as a completion checkpoint, and default multi-agent coordination for execution flows
- Rust automation guidance now uses tiered canonical grounding (Rust Book, Rust Reference, Cargo Book, Effective Rust) for disputed or safety-critical semantics
- Unified IPC protocol types: removed all `V2`/`V3` suffixes (`ControlRpcRequestV3` → `ControlRpcRequest`, `RuntimePayloadV2` → `RuntimePayload`, etc.); protocol version is now encoded only in the envelope `v` field
- Removed dead `DeviceType` variants (`EmotivInsight`, `EmotivEpocPlus`, `EmotivEpocX`, `Muse2`) from the types crate and all match arms
- Removed legacy config fields from `ServiceConfig` (`auto_start`, `ipc_simulation_enabled`, etc.) and corresponding CLI flags
- Hub FFT Plot and Time Series widgets now resolve sample rate from discovered stream metadata instead of using hardcoded 128 Hz
- macOS platform `get_cursor_position()` and `get_screen_info()` now use `enigo` APIs instead of stubs/hardcoded values; `check_accessibility_permission()` implemented using `AXIsProcessTrustedWithOptions`
- Removed commented-out `CGEvent`/`CGDisplay` code blocks from macOS platform module
- Python `Decoder` class docstring and `_ppo_update()` now clearly document that the training loop is a simplified REINFORCE prototype, not full PPO
- Security: `torch.load()` now uses `weights_only=True`; ErrP serialization migrated from `pickle` to `joblib`
- Cleaned up `#[allow(dead_code)]` and clippy suppression annotations across Rust crates
- Refactored `&Box<dyn DeviceProvider>` to `&dyn DeviceProvider` in device module
- Removed `mss` optional dependency from `neurohid-platform`
- Consolidated protocol docs into `docs/protocol-and-api.md` and architecture docs into `docs/architecture-rust-core.md`; removed redundant/stale documentation files
- Removed stale `_bmad/` directory reference from `docs/source-tree-analysis.md`

### Removed

- `docs/runtime-ml-protocol-v2.md` — deprecated V2 protocol document superseded by v3
- Merged `ipc_v2.rs` and `ipc_v3.rs` into unified `ipc.rs` in `neurohid-types`
- Legacy `.github/agents/**` directory and file-path based agent routing infrastructure; BMAD-native agent IDs and `_bmad/neurohid/workflows/neurohid-phase-workflow/workflow.md` are now authoritative
- Legacy in-tree Emotiv integration path (replaced by dedicated `emotiv-cortex-v2` and `emotiv-cortex-cli` crates in `https://github.com/jmduea/emotiv-cortex-rs`)
