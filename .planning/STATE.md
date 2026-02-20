# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-20)

**Core value:** A single, composable path from biosignal device to actionable output — with an IDE-like experience for building and training decoders and a standalone runtime for using them — so that developers and power users don't have to piece together disparate libraries and tools.
**Current focus:** Phase 4 — Standard path and recording

## Current Position

Phase: 4 of 6 (Standard path and recording)
Plan: 3 of 3 in current phase
Current Plan: 03
Total Plans in Phase: 3
Status: Plans 01, 02, and 03 complete (Phase 4 complete)
Last activity: 2026-02-20 — Plan 04-03 executed (XDF export, replay source, replay-offline, user-guide)

Progress: [██████████] 100%

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

### Pending Todos

None yet.

### Blockers/Concerns

None yet.

## Session Continuity

Last session: 2026-02-20
Stopped at: Plan 04-03 complete (SUMMARY, ROADMAP, STATE updated). Phase 4 complete. Next: Phase 5 (Hub-as-IDE) when started.
Resume file: None
