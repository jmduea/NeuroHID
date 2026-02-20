---
phase: 02-standalone-runtime-and-control
plan: 02
subsystem: runtime
tags: cli, control, ipc, neurohid-service, deployment

# Dependency graph
requires:
  - phase: 02-standalone-runtime-and-control
    provides: default control endpoint and standalone docs (02-01)
provides:
  - Control CLI: neurohid-service control snapshot | set-output-enabled
  - Runtime status and output toggle without Hub; scriptable control for developers
affects: deployment, scripting, COMP-03 verification

# Tech tracking
tech-stack:
  added: []
  patterns: one-shot blocking control client (send_control_request_blocking) for CLI

key-files:
  created: []
  modified:
    - crates/neurohid/src/bin/neurohid-service.rs
    - docs/deployment-guide.md

key-decisions:
  - "Control subcommand uses send_control_request_blocking for sync one-shot CLI exit"
  - "Default endpoint 127.0.0.1:47384 documented and used by control CLI"

patterns-established:
  - "Control CLI: client-only path (no runtime started); endpoint required via --endpoint with default"

requirements-completed:
  - RUNT-02
  - RUNT-03
  - COMP-03

# Metrics
duration: ~15min
completed: 2026-02-20
---

# Phase 2 Plan 02: Control CLI Summary

**Control CLI (snapshot, set-output-enabled) for neurohid-service with default endpoint 127.0.0.1:47384; status and output toggle without Hub; deployment guide updated.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- `neurohid-service control snapshot [--endpoint ADDR]` returns device_connected, decoder_ready, output_enabled, pipeline_integrity_degraded, integrity_issue_count.
- `neurohid-service control set-output-enabled <true|false> [--endpoint ADDR]` toggles action output; snapshot reflects change.
- Deployment guide documents Control CLI, default endpoint, and purpose (status/toggle without Hub, script control).

## Task Commits

Each task was committed atomically:

1. **Task 1: Add control subcommand to neurohid-service** - `f95ed50` (feat)
2. **Task 2: Document control CLI in deployment guide** - `85fffea` (docs)

## Files Created/Modified

- `crates/neurohid/src/bin/neurohid-service.rs` — ControlCommandCli (Snapshot, SetOutputEnabled), CliCommand::Control, run_control_command_sync using send_control_request_blocking.
- `docs/deployment-guide.md` — Control CLI subsection: commands, default endpoint, “without opening the Hub”, “script control”, link to protocol-and-api.

## Decisions Made

- Use blocking control client for control subcommand so the process exits after one request (no tokio runtime needed for client path).
- Default endpoint 127.0.0.1:47384 to match default standalone control server.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- RUNT-02, RUNT-03, COMP-03 satisfied for control via CLI.
- Phase 2 standalone runtime and control complete once this plan is marked done in ROADMAP.

## Self-Check: PASSED

- SUMMARY file present; task commits f95ed50, 85fffea verified in git log.

---
*Phase: 02-standalone-runtime-and-control*
*Completed: 2026-02-20*
