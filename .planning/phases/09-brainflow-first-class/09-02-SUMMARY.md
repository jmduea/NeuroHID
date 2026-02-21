---
phase: 09-brainflow-first-class
plan: 02
subsystem: ui
tags: [brainflow, lsl, hub, egui, runtime, sdk]

# Dependency graph
requires:
  - phase: 09-01
    provides: BrainFlow in default build, first-class docs
provides:
  - One runnable example (embedded_runtime) using BrainFlow synthetic end-to-end
  - Hub Devices and Settings copy with BrainFlow UX parity (discover/connect/disconnect)
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns: BrainFlow as peer to LSL in Hub copy; single runnable example per plan

key-files:
  created: []
  modified:
    - crates/neurohid-sdk/examples/embedded_runtime.rs
    - crates/neurohid-hub/src/screens/devices.rs
    - crates/neurohid-hub/src/screens/settings/mod.rs
    - crates/neurohid-hub/src/screens/settings/device.rs

key-decisions:
  - "embedded_runtime is the one runnable example; no new example crate"
  - "Devices/Settings copy only; discovery/connect implementation unchanged"

patterns-established:
  - "Multi-backend wording in Hub: streams (LSL, BrainFlow, …) not LSL-only"

requirements-completed: [BRAIN-02, BRAIN-03]

# Metrics
duration: 10
completed: "2026-02-21"
---

# Phase 09 Plan 02: BrainFlow Runnable Example and Hub UX Parity Summary

**Runnable embedded_runtime example using BrainFlow synthetic board and Hub Devices/Settings copy updated for BrainFlow as first-class (no LSL-only or parity-planned messaging).**

## Performance

- **Duration:** ~10 min
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- embedded_runtime example uses `DeviceBackend::BrainFlow` and `BrainFlowConfig::default()` (board_id 0, synthetic); runs with `cargo run -p neurohid-sdk --example embedded_runtime --features "runtime,types"` and shows streams=1.
- Devices screen: "Start service to discover/connect streams (LSL, BrainFlow, …)", heading "Available Streams", removed "LSL-first scope" and "Serial/BrainFlow parity is planned" chips; empty-state copy allows BrainFlow.
- Settings: removed "LSL-first telemetry UX" and "Serial/BrainFlow parity is phased later" chips; device.rs Serial/BrainFlow chips reworded to first-class copy.

## Task Commits

Each task was committed atomically:

1. **Task 1: Runnable example with BrainFlow synthetic** - `72af85a` (feat)
2. **Task 2: Hub Devices screen and Settings copy for BrainFlow parity** - `25a81e9` (feat)

## Files Created/Modified

- `crates/neurohid-sdk/examples/embedded_runtime.rs` - BrainFlow backend + BrainFlowConfig::default(), comment and streams count in output
- `crates/neurohid-hub/src/screens/devices.rs` - Multi-backend wording, removed LSL-first and parity chips
- `crates/neurohid-hub/src/screens/settings/mod.rs` - Removed LSL-first and Serial/BrainFlow parity chips
- `crates/neurohid-hub/src/screens/settings/device.rs` - First-class status copy for Serial and BrainFlow

## Decisions Made

- Use embedded_runtime as the single runnable example (no new example crate).
- Copy and status chips only in Hub; discover/connect/disconnect implementation unchanged.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- neurohid binary build failed with "Access is denied" when replacing neurohid-service.exe (file lock); neurohid-hub built successfully. Not caused by 09-02 changes.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- BRAIN-02 and BRAIN-03 satisfied; ready for 09-03.
- One runnable example and Hub UX parity in place.

## Self-Check: PASSED

- FOUND: .planning/phases/09-brainflow-first-class/09-02-SUMMARY.md
- FOUND: 72af85a, 25a81e9

---
*Phase: 09-brainflow-first-class*
*Completed: 2026-02-21*
