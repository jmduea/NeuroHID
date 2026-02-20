---
phase: 03-sdk-cli-for-device-and-pipeline-config
plan: 01
subsystem: sdk-cli
tags: [rust, neurohid-sdk, clap, device-discovery, control-ipc]

# Dependency graph
requires:
  - phase: 02-standalone-runtime-and-control
    provides: ControlCommand/RescanStreams/ConnectStream/Snapshot, send_control_request_blocking, neurohid-service
provides:
  - Public SDK device API (list streams via runtime or discovery-only, connect_by_id, connect_by_criteria, StreamConnectionHandle with drop=disconnect)
  - CLI device list and device connect subcommands (neurohid-service and neurohid entrypoint)
affects: Phase 4 (standard path), Phase 5 (Hub-as-IDE)

# Tech tracking
tech-stack:
  added: []
  patterns: SDK facade over RuntimeHandle/ControlCommand; CLI RescanStreams+Snapshot then table/JSON; neurohid binary dispatches to neurohid-service for device|config|pipeline|control|daemon

key-files:
  created: crates/neurohid-sdk/src/device/mod.rs, crates/neurohid-sdk/src/device/api.rs
  modified: crates/neurohid-sdk/src/lib.rs, crates/neurohid/src/bin/neurohid-service.rs, crates/neurohid/src/bin/neurohid.rs

key-decisions:
  - "Device module re-exports neurohid_device and adds high-level API (list_streams_via_runtime, connect_by_id, connect_by_criteria, StreamConnectionHandle)"
  - "neurohid binary dispatches to neurohid-service when first arg is device|config|pipeline|control|daemon; service binary located next to exe or in PATH"
  - "Exit codes: 0 success, 1 generic, 2 not found, 3 config invalid (documented in code)"

patterns-established:
  - "Device list: RescanStreams then Snapshot; human table or --json one-line; progress on stderr, -q suppresses"
  - "Connect by criteria: first match (stream_type or id contains); exit 2 when no stream matched"

requirements-completed: [COMP-01]

# Metrics
duration: 25min
completed: 2026-02-20
---

# Phase 3 Plan 01: SDK device API and CLI device list/connect Summary

**Public SDK device discovery/connection API (list, connect_by_id, connect_by_criteria, scoped handle) and CLI device list/connect with neurohid entrypoint dispatch.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 2
- **Files created:** 2 (device/mod.rs, device/api.rs)
- **Files modified:** 3 (lib.rs, neurohid-service.rs, neurohid.rs)

## Accomplishments

- SDK device module: list_streams_via_runtime(RuntimeHandle), list_streams_discovery(DeviceProvider), connect_by_id, connect_by_criteria; StreamConnectionHandle disconnects on drop
- neurohid-service: Device subcommand with List (--json, --quiet, --endpoint) and Connect (--device-id | --criteria); RescanStreams + Snapshot for list; exit 2 when stream not found
- neurohid binary: when first arg is device|config|pipeline|control|daemon, exec neurohid-service with rest of argv and exit with its code

## Task Commits

Each task was committed atomically:

1. **Task 1: SDK device API** - `0e7a2d6` (feat)
2. **Task 2: CLI device subcommands and neurohid dispatch** - `7ea173c` (feat)

## Files Created/Modified

- `crates/neurohid-sdk/src/device/mod.rs` - Device module re-export + high-level API surface
- `crates/neurohid-sdk/src/device/api.rs` - list_streams_via_runtime, connect_by_id, connect_by_criteria, list_streams_discovery, StreamConnectionHandle
- `crates/neurohid-sdk/src/lib.rs` - pub mod device (replaces re-export of neurohid_device; device module now includes both)
- `crates/neurohid/src/bin/neurohid-service.rs` - DeviceCommandCli (List, Connect), run_device_command_sync, fetch_discovered_streams
- `crates/neurohid/src/bin/neurohid.rs` - maybe_dispatch_to_service(), locate_service_binary(), CLI_SUBCOMMANDS

## Decisions Made

- Device API lives in neurohid-sdk device module alongside re-export of neurohid_device so existing MockProvider etc. remain at neurohid_sdk::device::*
- Connect by criteria uses first match (stream_type or id contains); order implementation-defined per CONTEXT
- neurohid locates neurohid-service next to current exe or in PATH; no GUI init when dispatching

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Clippy print_literal**
- **Found during:** Task 2 (device list table header)
- **Issue:** println!("... {}", "CHANNELS") triggered clippy::print_literal (literal with empty format)
- **Fix:** Use "CHANNELS" in format string instead of {}
- **Files modified:** neurohid-service.rs
- **Committed in:** 7ea173c (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (blocking)
**Impact on plan:** Fix required for -D warnings gate. No scope creep.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- COMP-01 satisfied: developer can drive device discovery, connection, and stream selection via SDK and CLI
- Plan 03-02 (config YAML, SDK config API, CLI config/pipeline) can proceed

## Self-Check: PASSED

- 03-01-SUMMARY.md present
- Commits 0e7a2d6 and 7ea173c present

---
*Phase: 03-sdk-cli-for-device-and-pipeline-config*
*Completed: 2026-02-20*
