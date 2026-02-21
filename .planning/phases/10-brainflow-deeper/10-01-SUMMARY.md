---
phase: 10-brainflow-deeper
plan: 01
subsystem: build, docs
tags: brainflow, native, reproducible-build, bash, uv

# Dependency graph
requires:
  - phase: 09-brainflow-first-class
    provides: BrainFlow default (synthetic) and docs/brainflow.md baseline
provides:
  - Pinned BrainFlow version (5.13.0) and full build order in docs/brainflow.md
  - Optional scripts/build-brainflow-native.sh for C++ build and lib copy
affects: 10-02 (real SDK behind feature flag)

# Tech tracking
tech-stack:
  added: scripts/build-brainflow-native.sh (bash, uv for Python)
  patterns: C++ → Rust → neurohid-device build order; optional script alongside canonical doc

key-files:
  created: scripts/build-brainflow-native.sh
  modified: docs/brainflow.md

key-decisions:
  - "Pinned BrainFlow tag 5.13.0 for reproducible native builds (BRAIN-08)"
  - "Canonical build order in docs; script optional and documented"

patterns-established:
  - "Single canonical doc (docs/brainflow.md) for version and build steps; script for convenience"

requirements-completed: [BRAIN-08]

# Metrics
duration: 2min
completed: 2026-02-21
---

# Phase 10 Plan 01: Pin BrainFlow and Document Build Order — Summary

**Pinned BrainFlow 5.13.0 and documented C++ → Rust → neurohid-device build order in docs/brainflow.md with optional reproducible-build script.**

## Performance

- **Duration:** ~2 min
- **Started:** 2026-02-21T20:49:39Z
- **Completed:** 2026-02-21T20:51:24Z
- **Tasks:** 2
- **Files modified:** 2 (1 created, 1 modified)

## Accomplishments

- docs/brainflow.md: added Native SDK (Phase 10) section with pinned tag 5.13.0, C++ (tools/build.py), Rust (copy installed/lib to rust_package/brainflow/lib), and neurohid-device build command; kept Phase 9 synthetic content.
- scripts/build-brainflow-native.sh: optional script using uv for Python, BRAINFLOW_REPO_DIR/BRAINFLOW_VERSION/BRAINFLOW_LIB_DIR, idempotent C++ build and lib copy; doc references it.

## Task Commits

Each task was committed atomically:

1. **Task 1: Document pinned version and build order in docs/brainflow.md** - `cd88c77` (docs)
2. **Task 2: Add optional reproducible-build script** - `9c82e09` (chore)

## Files Created/Modified

- `docs/brainflow.md` — Native SDK (Phase 10) subsection: pinned version, build order, optional script reference
- `scripts/build-brainflow-native.sh` — Optional script: run tools/build.py (uv), copy installed/lib to target

## Decisions Made

- Pinned BrainFlow git tag 5.13.0 as the canonical version for reproducible builds (BRAIN-08).
- Build order documented as authoritative in docs; script is optional and does not replace the doc.
- Use uv for Python in script and doc (project policy).

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- `cargo build -p neurohid` failed with "Access is denied" when removing target/debug/neurohid-service.exe (file in use). No code or feature changes were made; default build and features unchanged. Treated as environment/lock issue, not a regression.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- 10-02 can use docs/brainflow.md and the script as the foundation for adding the real SDK behind the brainflow-native feature flag.
- No blockers.

## Self-Check: PASSED

- FOUND: docs/brainflow.md, scripts/build-brainflow-native.sh, .planning/phases/10-brainflow-deeper/10-01-SUMMARY.md
- FOUND: commits cd88c77, 9c82e09

---
*Phase: 10-brainflow-deeper*
*Completed: 2026-02-21*
