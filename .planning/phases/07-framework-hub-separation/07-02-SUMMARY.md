---
phase: 07-framework-hub-separation
plan: 02
subsystem: infra
tags: ci, powershell, cargo, framework-boundary, allowlist

# Dependency graph
requires:
  - phase: 07-framework-hub-separation (plan 01)
    provides: .github/framework-allowlist.toml and docs/framework-surface.md
provides:
  - Script that enforces Hub and binaries path deps ⊆ allowlist
  - CI job that runs the boundary check on rust/automation/push
affects: crates (dependency changes to hub or neurohid will be gated)

# Tech tracking
tech-stack:
  added: []
  patterns: "CI boundary check via cargo metadata + single TOML allowlist"

key-files:
  created: .github/scripts/check-framework-boundary.ps1
  modified: .github/workflows/ci.yml

key-decisions:
  - "Boundary check runs on rust, automation, or push (allowlist/script changes trigger job)"
  - "No bypass or exception list; failures fixed by code or allowlist update"

patterns-established:
  - "Framework boundary: script reads allowlist from TOML, uses cargo metadata for path deps"

requirements-completed: [FRAME-03]

# Metrics
duration: 8min
completed: 2026-02-21
---

# Phase 07 Plan 02: Framework Boundary CI Summary

**Framework boundary enforced in CI via a PowerShell script that checks neurohid-hub and neurohid path dependencies against the canonical allowlist; CI job fails on violation with no permanent exceptions.**

## Performance

- **Duration:** ~8 min
- **Tasks:** 2
- **Files modified:** 2 (1 created, 1 modified)

## Accomplishments

- Dependency allowlist check script reads `.github/framework-allowlist.toml`, uses `cargo metadata` to get path deps for `neurohid-hub` and `neurohid`, and exits 1 with stderr when any path dep is not in the allowlist.
- CI job `framework-boundary` runs on rust impact, automation impact, or push; invokes the script with pwsh and fails the run on violation.

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement dependency allowlist check script** - `2fda804` (feat)
2. **Task 2: Add CI job for framework boundary check** - `976a890` (feat)

## Files Created/Modified

- `.github/scripts/check-framework-boundary.ps1` - Enforces hub and binaries path deps against allowlist; single source from TOML.
- `.github/workflows/ci.yml` - New job `framework-boundary` with rust/automation/push trigger.

## Decisions Made

- Job runs when `rust == 'true' || automation == 'true' || push` so that changes to the allowlist or script also run the check.
- No exception list or bypass in script or workflow; violations must be fixed by code or allowlist update.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## Next Phase Readiness

- FRAME-03 satisfied: Hub depends only on allowlisted crates; CI enforces no disallowed direct deps.
- Phase 7 plan 02 complete; framework boundary is enforced in CI.

## Self-Check: PASSED

- FOUND: .github/scripts/check-framework-boundary.ps1
- FOUND: .planning/phases/07-framework-hub-separation/07-02-SUMMARY.md
- FOUND: commits 2fda804, 976a890

---
*Phase: 07-framework-hub-separation*
*Completed: 2026-02-21*
