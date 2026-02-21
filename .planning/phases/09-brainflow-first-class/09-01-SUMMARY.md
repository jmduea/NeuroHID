---
phase: 09-brainflow-first-class
plan: 01
subsystem: docs, device-backends
tags: brainflow, neurohid-core, Cargo.toml, docs

# Dependency graph
requires: []
provides:
  - neurohid-core default features include brainflow (Hub/examples work without custom build)
  - Canonical BrainFlow doc (setup, config, synthetic vs native, build order)
  - docs/index.md link to brainflow.md
affects: 09-02, 09-03, 10-brainflow-native

# Tech tracking
tech-stack:
  added: []
  patterns: first-class device-backend doc linked from index

key-files:
  created: docs/brainflow.md
  modified: crates/neurohid-core/Cargo.toml, docs/index.md

key-decisions:
  - "BrainFlow enabled in default build path so Hub can select BrainFlow without --features"
  - "Single canonical doc docs/brainflow.md; BRAIN-04 (device-agnostic API) documented, no API change"

patterns-established:
  - "Device backend doc: setup, config, synthetic vs native, build order, API-preservation note"

requirements-completed: [BRAIN-01, BRAIN-04]

# Metrics
duration: 5min
completed: 2026-02-21
---

# Phase 09 Plan 01: BrainFlow First-Class Summary

**BrainFlow in default build and first-class docs: neurohid-core default features include brainflow; canonical docs/brainflow.md with setup, config, synthetic vs native, build order, and BRAIN-04 device-agnostic API note.**

## Performance

- **Duration:** ~5 min
- **Tasks:** 2
- **Files modified:** 3 (1 created, 2 modified)

## Accomplishments

- neurohid-core default features now include `brainflow` alongside `device-lsl`; Hub and examples get BrainFlow without `--features brainflow`.
- Created `docs/brainflow.md` as the canonical reference: setup (default build), config (BrainFlowConfig, board_id, serial_port), synthetic vs native (Phase 9 = simulation only; Phase 10 = native), build order (Phase 9 no C++ build).
- Documented that BrainFlow is one backend behind DeviceProvider/Device; device-agnostic API preserved (BRAIN-04).
- Linked brainflow.md from docs/index.md under Architecture and System Docs.

## Task Commits

Each task was committed atomically:

1. **Task 1: Enable BrainFlow in default build** - `2e3f412` (feat)
2. **Task 2: First-class BrainFlow documentation** - `a6b02a7` (docs)

## Files Created/Modified

- `docs/brainflow.md` - Canonical BrainFlow doc (setup, config, synthetic vs native, build order, API preservation).
- `crates/neurohid-core/Cargo.toml` - default = ["device-lsl", "brainflow"].
- `docs/index.md` - Link to brainflow.md in Architecture and System Docs.

## Decisions Made

- BrainFlow added to neurohid-core default features only (not neurohid-device or neurohid-sdk) so the crate that provides `create_provider` and is used by the Hub has BrainFlow by default.
- Single doc at docs/brainflow.md with link from index; no duplication in user-guide.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- `cargo build -p neurohid` reported "Access is denied" when replacing the built exe (likely process lock). Compilation succeeded; verification used `cargo build -p neurohid-core` for Task 2.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Hub can select BrainFlow in Settings without custom build.
- First-class BrainFlow doc exists and is linked; readers can find setup, config, synthetic vs native, and build-order guidance.
- Ready for 09-02 (runnable example) and 09-03 (Devices screen parity).

## Self-Check: PASSED

- docs/brainflow.md exists
- .planning/phases/09-brainflow-first-class/09-01-SUMMARY.md exists
- Commits 2e3f412 and a6b02a7 present in git log

---
*Phase: 09-brainflow-first-class*
*Completed: 2026-02-21*
