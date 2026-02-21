---
phase: 07-framework-hub-separation
plan: 01
subsystem: docs
tags: framework, hub, allowlist, toml, crate-boundaries

# Dependency graph
requires: []
provides:
  - docs/framework-surface.md (embedder-oriented framework surface and Hub boundary)
  - .github/framework-allowlist.toml (canonical hub/binaries allowlist for CI)
  - Links from docs/index.md, README.md, docs/crate-boundaries.md to framework boundary
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns: single source of truth for allowlist (TOML + doc reference)

key-files:
  created: .github/framework-allowlist.toml, docs/framework-surface.md
  modified: docs/index.md, README.md, docs/crate-boundaries.md

key-decisions:
  - "Framework surface and Hub allowlist documented in one canonical doc; allowlist file is single source for CI"
  - "Hub allowlist: neurohid-types, neurohid-core, neurohid-calibration, neurohid-storage, neurohid-ipc; no permanent exceptions"

patterns-established:
  - "Allowlist in .github/framework-allowlist.toml; docs/framework-surface.md references it so doc and script cannot diverge"

requirements-completed: [FRAME-01, FRAME-02, FRAME-04]

# Metrics
duration: 8min
completed: 2026-02-21
---

# Phase 7 Plan 01: Framework Surface and Hub Boundary Summary

**Single canonical framework surface and Hub allowlist with embedder-oriented doc and discoverable links from index, README, and crate-boundaries.**

## Performance

- **Duration:** ~8 min
- **Started:** 2026-02-21T19:21:42Z
- **Completed:** 2026-02-21
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Canonical allowlist in `.github/framework-allowlist.toml` for hub and binaries (CI single source of truth)
- Embedder-oriented `docs/framework-surface.md` with mental model, framework surface table, Hub boundary, and conceptual map
- Framework boundary discoverable from docs index, README (Architecture), and crate-boundaries (subsection linking to framework-surface)

## Task Commits

Each task was committed atomically:

1. **Task 1: Create framework allowlist and framework-surface doc** - `c7185c2` (feat)
2. **Task 2: Wire framework boundary into index, README, and crate-boundaries** - `c4a32d5` (feat)

## Files Created/Modified

- `.github/framework-allowlist.toml` - Canonical [hub] and [binaries] allowlists for CI
- `docs/framework-surface.md` - Framework surface, Hub boundary, guidance, conceptual map; references allowlist
- `docs/index.md` - Prominent link under Architecture and System Docs to framework-surface.md
- `README.md` - One-line link in Architecture for embedders
- `docs/crate-boundaries.md` - "Framework surface and Hub boundary" subsection pointing to framework-surface.md and allowlist

## Decisions Made

- Allowlist lives in TOML; framework-surface.md references it so CI script and doc stay in sync
- No permanent exceptions: re-export from core or update allowlist and code together
- Binaries crate allowlist includes hub allowlist plus neurohid-hub

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plan 07-02 (CI dependency allowlist check) can consume `.github/framework-allowlist.toml` and reference docs/framework-surface.md
- No blockers

## Self-Check: PASSED

- All created files present: .github/framework-allowlist.toml, docs/framework-surface.md, 07-01-SUMMARY.md
- Task commits present: c7185c2, c4a32d5

---
*Phase: 07-framework-hub-separation*
*Plan: 01*
*Completed: 2026-02-21*
