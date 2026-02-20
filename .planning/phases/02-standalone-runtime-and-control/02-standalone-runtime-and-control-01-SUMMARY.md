---
phase: 02-standalone-runtime-and-control
plan: 01
subsystem: runtime
tags: [neurohid-service, control-endpoint, tcp-loopback, deployment]

# Dependency graph
requires:
  - phase: 01-contracts-and-versioned-formats
    provides: versioned config/profile formats used by service
provides:
  - Default control port (47384) when standalone and config has no IPC endpoint
  - Documented standalone startup (profile, decoder-from-profile, control) without Hub
affects: [02-standalone-runtime-and-control, control CLI]

# Tech tracking
tech-stack:
  added: []
  patterns: [effective_control_port default when ipc_endpoint empty]

key-files:
  created: []
  modified: [crates/neurohid/src/bin/neurohid-service.rs, docs/deployment-guide.md]

key-decisions:
  - "Default standalone control port 47384 when service.ipc_endpoint empty and no --control-port"

patterns-established:
  - "Standalone default: compute effective_control_port from args and config; bind control on 127.0.0.1:47384 when endpoint empty"

requirements-completed: [RUNT-01]

# Metrics
duration: 15min
completed: 2026-02-20
---

# Phase 02 Plan 01: Standalone Service with Default Control Endpoint Summary

**Standalone runtime binds control server on 127.0.0.1:47384 by default when config has no IPC endpoint, with deployment guide documenting profile-based startup and control without Hub.**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-02-20 (plan execution)
- **Completed:** 2026-02-20
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Default control port 47384 when `service.ipc_endpoint` is empty and `--control-port` is not set; control server starts so status and output toggle are reachable without Hub.
- Deployment guide "Standalone runtime (without Hub)" section: profile implies decoder, startup options (`--config`, `--profile`, `--control-port`), default 127.0.0.1:47384, and control (snapshot, set_output_enabled) without opening the Hub.

## Task Commits

Each task was committed atomically:

1. **Task 1: Default control endpoint when running standalone** - `5f09d99` (feat)
2. **Task 2: Document standalone startup and control** - `e15307e` (docs)

## Files Created/Modified

- `crates/neurohid/src/bin/neurohid-service.rs` — Added `DEFAULT_STANDALONE_CONTROL_PORT`, `effective_control_port` logic, pass to `resolve_runtime_ipc_server_config`.
- `docs/deployment-guide.md` — New "Standalone runtime (without Hub)" section with profile/decoder, startup options, default port, and reference to Local Control Endpoint.

## Decisions Made

None — followed plan as specified. Default port 47384 was already specified in plan and research.

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None.

## Next Phase Readiness

- RUNT-01 satisfied: user can start standalone with chosen profile (decoder from profile) and have control reachable by default.
- Ready for 02-02 (control CLI for snapshot and set_output_enabled).

## Self-Check: PASSED

- SUMMARY.md present at `.planning/phases/02-standalone-runtime-and-control/02-standalone-runtime-and-control-01-SUMMARY.md`
- Task commits present: 5f09d99, e15307e

---
*Phase: 02-standalone-runtime-and-control*
*Completed: 2026-02-20*
