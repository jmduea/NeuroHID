# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-20)

**Core value:** A single, composable path from biosignal device to actionable output — with an IDE-like experience for building and training decoders and a standalone runtime for using them — so that developers and power users don't have to piece together disparate libraries and tools.
**Current focus:** Phase 5 — Hub-as-IDE

## Current Position

Phase: 6 of 6 (Composable and extensible)
Plan: 2 of 4 in current phase
**Current Plan:** 3
**Total Plans in Phase:** 4
Status: Plan 06-02 complete (Name-based selection and factories)
Last activity: 2026-02-21 — Plan 06-02 executed (device/outlet/signal/decoder config + factories, libloading, snapshot slot names)

Progress: [█████░░░░░] 50%

## Performance Metrics

**Velocity:**
- Total plans completed: 0
- Average duration: —
- Total execution time: 0 h

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

**Recent Trend:**
- Last 5 plans: —
- Trend: —

*Updated after each plan completion*
| Phase 01-contracts-and-versioned-formats P01 | 15 | 2 tasks | 3 files |
| Phase 02-standalone-runtime-and-control P01 | ~15 | 2 tasks | 2 files |
| Phase 03-sdk-cli-for-device-and-pipeline-config P01 | 25 | 2 tasks | 5 files |
| Phase 03-sdk-cli-for-device-and-pipeline-config P02 | ~25 | 3 tasks | 7 files |
| Phase 04 P01 | 5min | 1 task | 2 files |
| Phase 04 P02 | — | 3 tasks | 12+ files |
| Phase 05-hub-as-ide P02 | 15 | 2 tasks | 2 files |
| Phase 05-hub-as-ide P06 | 15 | 3 tasks | 7 files |
| Phase 06-composable-and-extensible P03 | 6 | 2 tasks | 9 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- (; roadmap just created)
- [Phase 01-contracts-and-versioned-formats]: Config format version and compatibility policy in same doc as schema (config-format.md); N=2 previous versions supported
- [Phase 01-contracts-and-versioned-formats]: Profile: calibration identity in metadata (no separate manifest) for export/import roundtrip; N=2 reader compatibility
- [Phase 03-sdk-cli-for-device-and-pipeline-config]: Device API in neurohid-sdk device module (list, connect_by_id, connect_by_criteria, StreamConnectionHandle); neurohid dispatches to neurohid-service for device|config|pipeline|control|daemon
- [Phase 03-sdk-cli-for-device-and-pipeline-config]: ConfigStore YAML/TOML by path extension; SDK config::load/save; CLI config show/validate, pipeline run --dry-run; exit 3 config invalid, --json errors to stderr
- [Phase 04-standard-path-and-recording]: User guide as dedicated doc (user-guide.md) with Standard path section; index links under Canonical Entry Points
- [Phase 04-standard-path-and-recording]: Recording config default_output_path as Option<String>; config snapshot as config.json in session folder; dispatch_control_request async for recording oneshot reply
- [Phase 05-hub-as-ide]: Lanes: Devices, Calibration, Training, Visualization, Config; Training screen stub for primary workflow
- [Phase 05-hub-as-ide]: Status bar shows Devices X/Y and Signal % from any screen; strip always visible (0/0 when stopped)
- [Phase 05-hub-as-ide]: Training screen split layout (config | live progress); Train on collected data stub; live metrics from ControlSnapshot + trainer_snapshot()
- [Phase 05-hub-as-ide]: Calibration game list/grid (Grid Maze, Target Tracking); single-game panel via new_for_game(kind); results persisted to active profile (HUB-02)
- [Phase 05-hub-as-ide]: Resume state as last_screen in UiConfig; Run in Hub / Run in background as user-facing labels (HUB-05)
- [Phase 06]: Extension identity by name only; duplicate names cause scan() to fail with ExtensionError::DuplicateName; default discovery path: config root + /extensions
- [Phase 06-02]: Device backend Extension(name); outlet/signal/decoder use optional extension_name in config; Loaded* wrappers hold libloading::Library + Box<dyn Trait>; snapshot exposes device/outlet/signal/decoder names
- [Phase 06]: Example outlet as workspace member (neurohid-outlet-example); e2e as in-process integration test asserting create_outlet and name (EXT-03)

### Pending Todos

None yet.

### Blockers/Concerns

None yet.

## Session Continuity

Last session: 2026-02-21
Stopped at: Completed 06-02-PLAN.md (Name-based selection and factories).
Resume file: None
