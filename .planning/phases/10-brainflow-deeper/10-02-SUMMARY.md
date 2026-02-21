---
phase: 10-brainflow-deeper
plan: 02
subsystem: device, build
tags: brainflow, native, feature-flag, Device, SampleStream

# Dependency graph
requires:
  - phase: 10-brainflow-deeper
    plan: "01"
    provides: Pinned BrainFlow 5.13.0 and build order in docs/brainflow.md
provides:
  - brainflow-native feature and optional BrainFlow crate in neurohid-device
  - Native BrainFlow Device/Provider mapping get_board_data to Sample, same pipeline as LSL
  - Docs: default/CI synthetic-only; native optional; same pipeline statement
affects: none (phase 10 complete)

# Tech tracking
tech-stack:
  added: brainflow (git 5.13.0, optional), num, tokio-util under brainflow-native
  patterns: cfg(brainflow-native) native path; Device → SampleStream → pipeline

key-files:
  created: crates/neurohid-device/src/brainflow_native.rs
  modified: crates/neurohid-device/Cargo.toml, crates/neurohid-device/src/brainflow.rs, crates/neurohid-device/src/lib.rs, docs/brainflow.md

key-decisions:
  - "brainflow-native not in default or neurohid-core; CI does not enable it"
  - "Native path uses same Device/SampleStream contract as LSL; no second pipeline"

patterns-established:
  - "Real SDK behind optional feature; synthetic path unchanged when feature off"

requirements-completed: [BRAIN-06, BRAIN-07]

# Metrics
duration: ~15min
completed: 2026-02-21
---

# Phase 10 Plan 02: Real BrainFlow SDK Behind Feature Flag — Summary

**Real BrainFlow SDK behind brainflow-native feature in neurohid-device; native Device/Provider maps get_board_data to Sample on the same Device → SampleStream pipeline as LSL; default and CI remain synthetic-only.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 3
- **Files modified:** 5 (1 created, 4 modified)

## Accomplishments

- brainflow-native feature and optional BrainFlow git dependency (tag 5.13.0); default features unchanged.
- Native Device and SampleStream in brainflow_native.rs: BoardShim prepare_session → start_stream → get_board_data loop mapped to Sample (timestamp_channel, eeg_channels, package_num_channel); stop_stream/release_session on stop_streaming/disconnect.
- brainflow.rs connect() uses native path when brainflow-native and (board_id != 0 or serial_port set); synthetic path unchanged otherwise.
- docs/brainflow.md: default and CI use synthetic only; native optional; BrainFlow path same pipeline as LSL (Device → SampleStream → pipeline).

## Task Commits

Each task was committed atomically:

1. **Task 1: Add brainflow-native feature and optional BrainFlow dependency** - `4cdd42c` (feat)
2. **Task 2: Implement native BrainFlow Device and SampleStream** - `37d316f` (feat)
3. **Task 3: Confirm default/CI and document native as optional** - `89d1dd2` (docs)

## Files Created/Modified

- `crates/neurohid-device/Cargo.toml` — brainflow-native feature, optional brainflow/num/tokio-util
- `crates/neurohid-device/src/brainflow_native.rs` — Native Device, SampleStream, connect_native, metadata
- `crates/neurohid-device/src/brainflow.rs` — connect() uses native path when brainflow-native and real board
- `crates/neurohid-device/src/lib.rs` — mod brainflow_native when brainflow + brainflow-native
- `docs/brainflow.md` — default/CI synthetic-only; same pipeline as LSL

## Decisions Made

- BrainFlow git tag `5.13.0` (no "v" prefix) — repo uses numeric tags.
- Native path only when board_id != 0 or serial_port set; board_id 0 stays synthetic.
- No re-export of BrainFlow types; all wrapped in Device/Sample.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] BrainFlow git tag format**
- **Found during:** Task 1 (Add brainflow-native feature)
- **Issue:** Cargo failed with "failed to find tag `v5.13.0`"; BrainFlow repo uses tag `5.13.0` without "v".
- **Fix:** Updated Cargo.toml to `tag = "5.13.0"`.
- **Files modified:** crates/neurohid-device/Cargo.toml
- **Committed in:** 4cdd42c (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (blocking)
**Impact on plan:** Necessary for dependency resolution; no scope creep.

## Issues Encountered

- `cargo build -p neurohid` failed with "Access is denied" removing neurohid-service.exe (file in use). Same environment issue as 10-01; not caused by this plan. Treated as out-of-scope.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 10 plan 02 complete. BRAIN-06 and BRAIN-07 satisfied.
- No blockers. Native build requires user to build BrainFlow C++ then Rust per docs/brainflow.md.

## Self-Check

- FOUND: crates/neurohid-device/Cargo.toml, brainflow_native.rs, brainflow.rs, lib.rs, docs/brainflow.md
- FOUND: commits 4cdd42c, 37d316f, 89d1dd2

---
*Phase: 10-brainflow-deeper*
*Completed: 2026-02-21*
