---
phase: 01-contracts-and-versioned-formats
plan: 03
subsystem: docs
tags: [LSL, stream-semantics, timestamps, pull_sample, COMP-04]

# Dependency graph
requires: []
provides:
  - docs/formats/stream-semantics.md (consumption, timestamps, latest-sample, overflow per stream type)
  - LSL device module doc linking to stream-semantics.md
affects: [SDK, CLI, runtime, decoder pipeline]

# Tech tracking
tech-stack:
  added: []
  patterns: [structured format doc with BNF/schema + prose; code reference to spec]

key-files:
  created: [docs/formats/stream-semantics.md]
  modified: [crates/neurohid-device/src/lsl/device.rs]

key-decisions:
  - "LSL runtime behavior documented as continuous pull (pull_sample(0.2)), every sample forwarded; drain-then-last defined as alternative for consumers who want only latest"
  - "Single stream-semantics doc covers LSL in full and Serial/BrainFlow/Mock briefly"

patterns-established:
  - "Stream semantics: one doc per stream contract with consumption, timestamps, latest-sample, overflow"
  - "Implementation references spec in module doc; no behavior change in doc-only phase"

requirements-completed: [COMP-04]

# Metrics
duration: 15min
completed: "2026-02-20"
---

# Phase 01 Plan 03: Stream Semantics Summary

**Stream consumption, timestamps, and latest-sample semantics documented for LSL (full) and Serial/BrainFlow/Mock (brief); LSL device references spec; COMP-04 satisfied.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 2
- **Files modified:** 2 (1 created, 1 modified)

## Accomplishments

- Added `docs/formats/stream-semantics.md` with BNF-style overview and prose: LSL consumption (blocking `pull_sample(timeout)`, non-blocking `pull_sample(0.0)` → 0.0), timestamps (remote capture → μs, system_timestamp at receive), latest-sample (drain-then-last defined; NeuroHID = continuous pull 0.2 s, every sample forwarded), overflow (max_buflen).
- Short subsections for Serial, BrainFlow, Mock (consumption, timestamps, latest-sample, overflow where applicable).
- LSL device module doc now references `docs/formats/stream-semantics.md` and states continuous `pull_sample(0.2)` and every-sample-forward behavior; no code or behavior change.

## Task Commits

1. **Task 1: Write stream semantics document (LSL full, others brief)** — `383e779` (feat)
2. **Task 2: Align LSL device code with doc and add reference** — `4ea7457` (feat)

## Files Created/Modified

- `docs/formats/stream-semantics.md` — Stream semantics: consumption model, timestamps, latest-sample, overflow/drops for LSL (full) and Serial/BrainFlow/Mock (brief).
- `crates/neurohid-device/src/lsl/device.rs` — Module doc references stream-semantics.md; documents continuous pull_sample(0.2) and every-sample forward.

## Decisions Made

- Documented NeuroHID LSL behavior as continuous pull with 0.2 s timeout, every sample forwarded; "latest sample" = most recently received. Drain-then-last described as the alternative for consumers who want only the latest sample per tick.
- Single doc for all stream types with LSL in full and others brief to keep one place for COMP-04 and future SDK/CLI consistency.

## Deviations from Plan

None — plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- COMP-04 satisfied; stream semantics are documented and LSL implementation is aligned.
- Phase 1 plans 01 and 02 (config/profile versioning, calibration identity) can proceed independently; plan 03 is complete.

## Self-Check: PASSED

- FOUND: docs/formats/stream-semantics.md
- FOUND: .planning/phases/01-contracts-and-versioned-formats/01-03-SUMMARY.md
- FOUND: commits 383e779, 4ea7457

---
*Phase: 01-contracts-and-versioned-formats*
*Plan: 03*
*Completed: 2026-02-20*
