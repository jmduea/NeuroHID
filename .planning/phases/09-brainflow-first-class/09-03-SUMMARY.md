---
phase: 09-brainflow-first-class
plan: 03
subsystem: device
tags: [brainflow, device-backend, discovery, mock, synthetic]

# Dependency graph
requires:
  - phase: 09-brainflow-first-class (09-01)
    provides: BrainFlow in default build, first-class docs
provides:
  - Auto backend fallback = BrainFlow synthetic (board_id 0)
  - Single non-hardware path: tests, CI, examples use BrainFlow synthetic
  - Mock retained only for explicit DeviceBackend::Mock use
affects: [CI, examples, validation harness, SDK tests]

# Tech tracking
tech-stack:
  added: []
  patterns: [Auto provider fallback to BrainFlow, test config BrainFlowConfig::default()]

key-files:
  created: []
  modified:
    - crates/neurohid-core/src/tasks/device/discovery.rs
    - crates/neurohid-core/src/tasks/device/mod.rs
    - crates/neurohid-core/src/runtime.rs
    - crates/neurohid/src/bin/neurohid-service.rs
    - crates/neurohid-sdk/src/lib.rs
    - crates/neurohid-hub/src/service_manager/tests.rs
    - crates/neurohid/src/bin/neurohid-validate.rs

key-decisions:
  - "Auto fallback is BrainFlow synthetic (board_id 0); Mock not used in Auto path"
  - "SDK device tests that need a device require device-brainflow feature"

patterns-established:
  - "Auto provider: LSL first, then fallback.discover()/connect() with BrainFlow synthetic"
  - "Tests and binaries use DeviceBackend::BrainFlow + brainflow: Some(BrainFlowConfig::default()) for non-hardware path"

requirements-completed: [BRAIN-05]

# Metrics
duration: ~15min
completed: 2026-02-21
---

# Phase 09 Plan 03: BrainFlow Synthetic as Single Non-Hardware Path Summary

**Auto backend falls back to BrainFlow synthetic (board_id 0); all tests, binaries, and CI use BrainFlow synthetic instead of Mock; BRAIN-05 satisfied.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments

- Auto backend uses `create_brainflow_provider(config)` as fallback instead of MockProvider; logs reference "BrainFlow synthetic"
- AutoProvider connect() tries LSL then fallback (removed mock_ prefix special-case)
- All runtime, service, validate, and Hub service_manager tests use DeviceBackend::BrainFlow with BrainFlowConfig::default()
- SDK device tests use BrainFlowProvider/BrainFlowConfig, gated on device-brainflow feature
- DeviceBackend::Mock and discovery Mock branch unchanged for explicit Mock use

## Task Commits

Each task was committed atomically:

1. **Task 1: Auto fallback to BrainFlow synthetic** - `e67c4c8` (feat)
2. **Task 2: Replace Mock with BrainFlow synthetic in all tests and binaries** - `4322d0f` (feat)

## Files Created/Modified

- `crates/neurohid-core/src/tasks/device/discovery.rs` - Auto branch uses BrainFlow fallback; AutoProvider fallback field and log messages
- `crates/neurohid-core/src/tasks/device/mod.rs` - Doc: Auto falls back to BrainFlow synthetic
- `crates/neurohid-core/src/runtime.rs` - Tests use BrainFlow + BrainFlowConfig::default()
- `crates/neurohid/src/bin/neurohid-service.rs` - Test configs use BrainFlow + brainflow config
- `crates/neurohid-sdk/src/lib.rs` - Device tests use BrainFlowProvider/BrainFlowConfig; gated on device-brainflow
- `crates/neurohid-hub/src/service_manager/tests.rs` - Configs use BrainFlow + BrainFlowConfig::default()
- `crates/neurohid/src/bin/neurohid-validate.rs` - build_config uses BrainFlow + BrainFlowConfig::default()

## Decisions Made

- SDK device tests that use BrainFlowProvider are gated on `device-brainflow` so neurohid-device is built with brainflow feature
- Mock remains in enum and discovery branch; only default/test paths switched to BrainFlow

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- runtime.rs test module has its own `use`; added BrainFlowConfig to the test module's imports (not a deviation; correct scoping)
- neurohid build failed with "Access is denied" on exe (file lock); compilation succeeded

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- BRAIN-05 complete; BrainFlow synthetic is the single non-hardware path in tests and CI
- Phase 09 plan 03 complete; ready for remaining phase work or phase close-out

## Self-Check

- SUMMARY.md created: FOUND
- Task commits e67c4c8, 4322d0f present in repo: verified via git log

---
*Phase: 09-brainflow-first-class*
*Completed: 2026-02-21*
