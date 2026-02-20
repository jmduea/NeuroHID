---
phase: 04-standard-path-and-recording
plan: 01
subsystem: docs
tags: user-guide, standard-path, PATH-01

# Dependency graph
requires:
  - phase: 03-sdk-cli-for-device-and-pipeline-config
    provides: device list/connect, config show/validate, pipeline run --dry-run, config-format
provides:
  - Single user-facing path document: device → decoder → run
  - docs/index.md link to user guide under Canonical Entry Points
affects: Phase 4 recording/replay docs will reference same path

# Tech tracking
tech-stack:
  added: []
  patterns: One-doc standard path with optional branches; link to deployment for ops

key-files:
  created: docs/user-guide.md
  modified: docs/index.md

key-decisions:
  - "User guide as dedicated doc (user-guide.md) with Standard path section; index links under Canonical Entry Points"

patterns-established:
  - "Standard path doc: single walkthrough, informal tone, optional branches; ops/transport in deployment-guide"

requirements-completed: [PATH-01]

# Metrics
duration: 5min
completed: "2026-02-20"
---

# Phase 04 Plan 01: Standard Path (User Guide) Summary

**Single documented path from device to decoder to actions in docs/user-guide.md, linked from docs index.**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-02-20T21:29:50Z
- **Completed:** 2026-02-20
- **Tasks:** 1
- **Files modified:** 2

## Accomplishments

- Added `docs/user-guide.md` with section **Standard path: from device to actions** (device in hand → pick/connect device → pick decoder config+profile → run).
- Referenced CLI `device list` / `device connect`, `config show` / `config validate`, `pipeline run --dry-run`, and `neurohid-service` with defaults; linked to config-format and deployment-guide.
- Informal tone and optional branches (LSL, Hub, control without Hub, advanced config).
- Updated `docs/index.md` with canonical entry "User guide: standard path and workflows" linking to user-guide.md.

## Task Commits

Each task was committed atomically:

1. **Task 1: User guide and standard path section** - `d91f521` (feat)

**Plan metadata:** (final commit to add SUMMARY/STATE/ROADMAP will follow)

## Files Created/Modified

- `docs/user-guide.md` — User guide with Standard path section (device → decoder → run), CLI/SDK references, links to deployment-guide and config-format.
- `docs/index.md` — Added User guide entry under Canonical Entry Points.

## Decisions Made

- User guide as a dedicated doc with one clear purpose (standard path); index reflects new doc per docs AGENTS.md. No separate "reproducibility" subsection (reproducibility is side effect of recording/export per CONTEXT).

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- PATH-01 satisfied: users can follow docs/index → user guide → one coherent path from device to decoder to actions.
- Ready for 04-02 (session recording) and 04-03 (export/replay); path doc can be referenced from recording/replay sections.

## Self-Check: PASSED

- FOUND: docs/user-guide.md
- FOUND: commit d91f521

---
*Phase: 04-standard-path-and-recording*
*Completed: 2026-02-20*
