---
phase: 08-thorough-testing
plan: 03
subsystem: testing
tags: [ci, coverage, pytest, nextest, documentation]

# Dependency graph
requires: []
provides:
  - Coverage gate numbers in docs aligned with ci.yml (50/35)
  - Retries and flakiness policy documented; CI = safe-to-merge for scope
affects: [08-01, 08-05]

# Tech tracking
tech-stack:
  added: []
  patterns: [doc-as-single-source-of-truth for coverage gates]

key-files:
  created: []
  modified: [docs/development-guide.md]

key-decisions:
  - "Coverage thresholds: Python 50%, Rust 35% — doc points to ci.yml env as source of truth"
  - "Retries only for identified flaky tests; broad reruns avoided; CI reflects reality"

patterns-established:
  - "Development guide coverage section kept in sync with .github/workflows/ci.yml env"

requirements-completed: [TEST-03]

# Metrics
duration: 5min
completed: 2026-02-21
---

# Phase 08 Plan 03: CI–Docs Alignment Summary

**Coverage gates (Python 50%, Rust 35%) and retries/flakiness policy documented in development-guide; doc points to ci.yml as source of truth.**

## Performance

- **Duration:** 5 min
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments

- development-guide.md coverage section updated to PYTHON_COVERAGE_MIN = 50 and RUST_COVERAGE_MIN = 35, with note that ci.yml env is source of truth.
- New "Retries and flakiness policy" subsection: retries only for identified flaky tests; broad reruns avoided; CI passing = safe-to-merge for scope exercised; pointer to test tiers doc when available.

## Task Commits

Each task was committed atomically:

1. **Task 1: Sync development-guide coverage numbers with ci.yml** - `8a606a8` (docs)
2. **Task 2: Document retries and flakiness policy in development-guide** - `18a3e92` (docs)

## Files Created/Modified

- `docs/development-guide.md` - Coverage gates 50/35, source-of-truth note, retries/flakiness policy subsection

## Decisions Made

- Doc states 50/35 and defers current values to ci.yml env.
- Retries and flakiness policy documented without cross-file to testing.md (file not present); generic pointer to "test tiers doc (when available)" used.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- TEST-03 satisfied; docs and CI aligned; flakiness policy documented.
- Ready for subsequent Phase 8 plans (e.g. 08-05 test tiers doc can add testing.md and development-guide can link to it).

## Self-Check: PASSED

- FOUND: .planning/phases/08-thorough-testing/08-03-SUMMARY.md
- FOUND: 8a606a8, 18a3e92

---
*Phase: 08-thorough-testing*
*Completed: 2026-02-21*
