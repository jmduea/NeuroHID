# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Canonical automation backbone: `.github/automation/scope-map.json`, impact classifier (`.github/scripts/classify-impact.ps1`), local/CI quality runner (`.github/scripts/run-agent-ready-tasks.ps1`), and policy validators for docs freshness, unsafe compliance, and protocol contracts
- Architecture index automation via `.github/scripts/generate-architecture-index.ps1` with tracked output at `docs/architecture/index.md`
- CI enhancements for impact-aware job routing, focused gate execution, unsafe compliance, protocol contract validation, and harness smoke report artifact publishing
- Executable protocol-documentation contract test in `neurohid-types` (`ipc_v2` tests)
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
- Visualization migration cookbook with phased `armas` and constrained `egui_dock` adoption guidance (`docs/ux/egui-visual-migration-cookbook.md`)
- Default multi-agent phase workflow contract at `.github/agents/_shared/multi-agent-phase-workflow.md` with routing precedence and completion-phase artifacts
- Agent routing integrity workflow `.github/workflows/agent-routing-integrity.yml` with hook schema checks, route integrity checks, and fixture-based regression checks
- Hook policy validators: `.github/hooks/validate-routing.ps1`, `.github/hooks/test-validate-routing.ps1`, and `.github/hooks/validate-doc-contracts.ps1`

### Changed

- Reorganized project into Rust workspace with separate published and internal crates
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
- Python Lab now presents a chip-based execution summary (kernel/cell/queue/outcome counts) and clearer action hierarchy (primary run actions, secondary utilities, ghost destructive/cleanup) for faster notebook workflow scanning
- Dashboard diagnostics and ML Bridge sections now use denser chip-based health summaries and aligned toggle/action rows, improving runtime/trainer status scanability without changing control behavior
- Settings now includes an always-visible configuration summary (save state/runtime/UI/backend/notifications) plus lightweight category cues across collapsible sections to improve information architecture and reduce scan time
- Devices screen now surfaces stream/signal/route health chips, replaces color-only state bullets with explicit status chips, and applies clearer primary vs ghost connect/disconnect action emphasis for faster connection triage
- Profiles screen now includes a top-level profile health summary (count/calibration/active state), upgraded status badges via shared chips, and consistent primary/secondary/ghost action emphasis across create/activate/delete flows
- Jupyter IDE screen now uses chip-based environment/session status plus clearer action hierarchy (primary prepare/start, secondary open, ghost stop/clear) for quicker managed-lab operations
- Calibration entry now surfaces readiness chips (service/device/signal), explicit calibration-status chips for active profile context, and unified warning/status messaging before launch
- Stream Console header/footer now uses explicit live/paused + buffer/match status chips and clearer action emphasis, improving readability during high-throughput monitoring
- Calibration panel signal-check/welcome phases now include explicit progress visuals and clearer textual quality states (good/fair/low) to improve pre-game calibration guidance
- Visualization toolbar now uses explicit status chips for rate/buffer/connection/staleness/elapsed context with reduced separator clutter, improving live situational scanability without changing pane/layout behavior
- Dashboard now replaces remaining color-only warning/error/outcome lines with explicit status chips for runtime constraints, training state, bridge/trainer alerts, and candidate outcomes
- Visualization layout manager now includes explicit docking guidance/status chips and warning-chip fallbacks for missing pane widget instances, improving pane-level affordance clarity
- Final hub consistency sweep converted remaining high-visibility color-only statuses (Python Lab cell/bridge state, Devices quality labels, app init error banner) to shared status-chip semantics
- Minor follow-up consistency pass converted residual inline warning/error labels in Settings, Profiles delete confirmation, and Dashboard task-error remediation to shared status chips
- Focus widget headline now uses shared status-chip semantics for focus percentage state while retaining existing graph/trend color rendering
- Devices empty-state guidance now uses warning chips (plus concise helper text), and Profiles calibration badges now map directly from calibration state to intent without color-based inference
- Visualization offline welcome panel now exposes explicit service/data status chips, and Stream Metadata widget now includes discovered/connected summary chips plus warning-chip empty state
- Widget empty states now consistently use warning/info status chips across Time Series, Spectrogram, Signal Quality, Action Preview, and Headplot when waiting for data/samples
- Focus widget waiting state now also uses the shared warning chip, completing waiting-state consistency across core visualization widgets
- Devices/Calibration/Profiles precondition and empty states now use the same chip-first warning pattern with concise helper text, improving startup-path consistency across screens
- Additional widget consistency pass now applies status-chip empty states for Accelerometer, Decoder Monitor, Action Preview log states, Spectrogram row absence, and Time Series channel-disable states
- FFT Plot and Band Power widgets now render collecting-data placeholders as warning status chips for consistency with other visualization empty states
- Dashboard trainer-disconnect helper text and Stream Metadata source-id rows now use chip-based cues for more consistent operational scanning
- Hub status bar stopped-state indicator now uses shared muted status-chip semantics instead of a custom color-blended badge
- Dashboard Diagnostics and ML Trainer detail lines now render runtime/model/capability/protocol context as chips for denser operational scanning
- Visualization welcome/footer guidance, docking instructions, and Stream Console per-stream summary now use shared chip cues for helper-text consistency
- Widget-level helper/subheader metadata in Headplot, Spectrogram, Accelerometer, and Signal Quality now uses muted chips for cross-widget consistency
- Wrap-heavy chip labels in Dashboard/Layout/Visualization/Stream Console were compacted for denser readability on smaller panel widths
- Remaining screen precondition/empty-state helper lines (Devices/Calibration/Profiles) and dashboard task-error detail now use chip-first status semantics
- Settings helper guidance (save tip, runtime mode notes, docking/Jupyter hints, LSL predicate example) now uses chip-first hint semantics for copy consistency
- Jupyter IDE command metadata lines (bootstrap/jupyter/url) now render as compact chips for better scanability and consistency with the chip-first operational UI style
- Stream Console footer now presents lines/rate/buffer/stream metrics as compact chips (with optional per-stream detail text) instead of a single dense status sentence
- Dashboard diagnostics now renders trainer-snapshot unavailable state as a muted status chip plus concise helper text for message hierarchy consistency
- Calibration wizard now includes explicit step count and progress bar (in addition to dot indicators) for clearer multi-step orientation during setup
- Stream Metadata grid now uses explicit battery/connected status chips for faster row-level health scanning while preserving compact table density
- Repo automation routing was consolidated to existing agent inventory with writer-owned documentation freshness, completion-finisher as a completion checkpoint, and default multi-agent coordination for execution flows
- Rust automation guidance now uses tiered canonical grounding (Rust Book, Rust Reference, Cargo Book, Effective Rust) for disputed or safety-critical semantics

### Removed

- Legacy in-tree Emotiv integration path (replaced by dedicated `emotiv-cortex-v2` and `emotiv-cortex-cli` crates in `https://github.com/jmduea/emotiv-cortex-rs`)
