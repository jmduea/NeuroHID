---
phase: 04-standard-path-and-recording
plan: 01
type: execute
tags: [PATH-01, user-guide, standard-path, docs]

# Dependency graph
requires:
  - phase: 03 (SDK/CLI device and config)
provides:
  - Single documented path: device → decoder → run (PATH-01)
  - docs/user-guide.md with "Standard path: from device to actions" section
  - docs/index.md canonical entry linking to user guide
affects: Phase 4 success criteria (PATH-01); later plans unchanged

# Tech tracking
tech-stack:
  added: []
  patterns: one user-facing doc, link from index (Canonical Entry Points)

key-files:
  created: docs/user-guide.md
  modified: docs/index.md

key-decisions:
  - (from CONTEXT) User guide as dedicated doc; Standard path section; informal tone; optional branches; link to deployment-guide for transport/control

requirements-completed: [PATH-01]

# Metrics
duration: 0
completed: 2026-02-20
---

# Phase 4 Plan 1: User Guide and Standard Path Summary

**Single coherent path (PATH-01): one user-facing document from device in hand to decoder driving actions, with minimal assumptions and optional branches. Linked from docs index.**

## What was built

- **docs/user-guide.md** — User guide with section **"Standard path: from device to actions"**:
  - Walkthrough: (1) device in hand → (2) pick/connect device (CLI `device list` / `device connect`, SDK reference) → (3) pick decoder (config + profile; config-format, pipeline run --dry-run) → (4) run (neurohid-service or pipeline run with defaults).
  - Informal tone; optional branches (LSL, Hub GUI, control without Hub, advanced config).
  - Links to [deployment-guide](deployment-guide.md) for transport, control, observability; no duplicate runbooks.
- **docs/index.md** — Canonical entry under "Canonical Entry Points": **"User guide: standard path and workflows"** → [User guide](user-guide.md).

## Verification

- `docs/user-guide.md` exists and contains the standard path section.
- `docs/index.md` contains a link to `user-guide.md` (user-facing entry).
- `rg -i "standard path|device.*decoder|user-guide" docs/` shows expected content in user-guide.md and index.md.
- No separate "reproducibility" subsection (reproducibility is side effect of recording/export per CONTEXT).

## Task / Commit

- **Task 1: User guide and standard path section** — Delivered in commit `d91f521` (feat(04-01): user guide and standard path section (PATH-01)). This execution run verified artifacts and added SUMMARY/ROADMAP/STATE.

## Deviations from plan

None. Content and index link were already present; verification confirmed and planning artifacts added.

## Success criteria

- PATH-01 satisfied: User can follow documented steps from "device in hand" to "decoder driving actions" using defaults and one coherent path.
- Document linked from docs index; informal tone with optional branches.

---
*Phase: 04-standard-path-and-recording*
*Plan: 01*
*Completed: 2026-02-20*
