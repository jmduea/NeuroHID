---
phase: 08-thorough-testing
plan: 05
subsystem: testing
tags: testing, unit, integration, e2e, isolation, flakiness, nextest, pytest

# Dependency graph
requires: []
provides:
  - docs/testing.md as single source for test tiers and isolation policy
  - development-guide link so contributors find test policy from one place
affects: Phase 8 (Thorough Testing), contributors writing or running tests

# Tech tracking
tech-stack:
  added: []
  patterns: documented condition-based wait, ephemeral ports, config roundtrip; retries policy

key-files:
  created: docs/testing.md
  modified: docs/development-guide.md

key-decisions:
  - "Single doc docs/testing.md for tier definitions and isolation; development-guide links only (no duplication)"

patterns-established:
  - "Test tiers (unit / integration / E2E) and isolation policy documented in one canonical doc"

requirements-completed: [TEST-05]

# Metrics
duration: 5
completed: "2026-02-21"
---

# Phase 08 Plan 05: Test Tiers and Isolation Doc Summary

**Test tiers (unit, integration, E2E) and isolation policy documented in docs/testing.md with development-guide link for contributors.**

## Performance

- **Duration:** ~5 min
- **Tasks:** 2 completed
- **Files modified:** 2 (created 1, modified 1)

## Accomplishments

- Created `docs/testing.md` with tier definitions (unit, integration, E2E), isolation policy (ports, dirs, env, IPC), and flakiness avoidance (condition-based wait, retries policy).
- Linked from `docs/development-guide.md` under Validation and Testing so contributors can find test tiers and how to avoid flakiness from one place.

## Task Commits

Each task was committed atomically:

1. **Task 1: Create docs/testing.md with tiers and isolation policy** - `7553b94` (feat)
2. **Task 2: Link from development-guide to testing doc** - `79fc60e` (docs)

## Files Created/Modified

- `docs/testing.md` - Tier definitions, isolation policy, flakiness avoidance; references neurohid-service wait_for_runtime_start, neurohid-ipc allocate_test_port, neurohid-storage save_then_load_roundtrip
- `docs/development-guide.md` - Link to testing.md for test tiers and isolation

## Decisions Made

- Single source: `docs/testing.md` holds all tier and isolation content; development-guide only links (no duplication), per plan and docs AGENTS.md.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## Next Phase Readiness

TEST-05 satisfied. Contributors can read what is unit vs integration vs E2E and how to avoid flakiness; isolation policy (ports, dirs, env, IPC) and retries policy are documented. No contradiction with nextest.toml (08-01) or coverage docs (08-03).

## Self-Check: PASSED

- FOUND: docs/testing.md
- FOUND: .planning/phases/08-thorough-testing/08-05-SUMMARY.md
- FOUND: 7553b94, 79fc60e in git log

---
*Phase: 08-thorough-testing*
*Completed: 2026-02-21*
