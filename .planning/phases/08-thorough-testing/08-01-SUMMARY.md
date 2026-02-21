---
phase: 08-thorough-testing
plan: 01
subsystem: testing
tags: [nextest, ci, rust, retries, timeout]
requires: []
provides:
  - CI Test and Test (macOS) jobs run cargo nextest; workspace and extension_outlet_e2e under nextest
  - nextest.toml at repo root with retries and slow-timeout for deterministic baseline
affects: [08-02, 08-03, 08-05]
tech-stack:
  added: [cargo-nextest, taiki-e/install-action, nextest.toml]
  patterns: [nextest as single test runner in Test jobs, minimal retries so flakiness visible]
key-files:
  created: [nextest.toml]
  modified: [.github/workflows/ci.yml]
key-decisions:
  - "Use cargo-nextest@0.9 in CI via taiki-e/install-action; rust-coverage job unchanged (08-RESEARCH)"
  - "nextest.toml at repo root with retries (fixed, 2) and slow-timeout (60s period, terminate-after 3)"
patterns-established:
  - "Test job: install nextest then cargo nextest run --workspace; separate step for extension_outlet_e2e via nextest"
  - "Config: slow-timeout with period + terminate-after; retries with fixed backoff, minimal count"
requirements-completed: [TEST-01]
duration: 15
completed: 2026-02-21
---

# Phase 08 Plan 01: Nextest and nextest.toml Summary

**Rust tests run via cargo-nextest in CI with retries and slow-timeout; repo-root nextest.toml provides deterministic, bounded test runs (TEST-01).**

## Performance

- **Duration:** ~15 min
- **Tasks:** 2
- **Files created:** nextest.toml
- **Files modified:** .github/workflows/ci.yml

## Accomplishments

- Test and Test (macOS) jobs install cargo-nextest (taiki-e/install-action, cargo-nextest@0.9) and run `cargo nextest run --workspace` and `cargo nextest run -p neurohid-core --test extension_outlet_e2e`.
- nextest.toml at repo root with retries (backoff = "fixed", count = 2) and slow-timeout (period = "60s", terminate-after = 3); policy comments reference 08-RESEARCH and docs/testing.md.
- rust-coverage job left unchanged per plan and 08-RESEARCH.

## Task Commits

1. **Task 1: Add nextest and switch CI Test jobs to nextest** - `c303999` (feat)
2. **Task 2: Add nextest.toml with retries and timeout** - `a4ea6b4` (chore)

## Files Created/Modified

- `nextest.toml` - Retries and slow-timeout for default profile; comments for policy and references.
- `.github/workflows/ci.yml` - Install cargo-nextest in Test and Test (macOS); replace `cargo test` with `cargo nextest run` for workspace and extension_outlet_e2e.

## Decisions Made

- Use `slow-timeout` (not `run.tests.timeout`) in nextest.toml to match current nextest config schema (see Deviations).
- Pin nextest in CI to 0.9.x via `cargo-nextest@0.9` in install-action.
- Keep rust-coverage job as-is; no nextest in coverage path for this plan.

## Deviations from Plan

### Auto-fixed / Correctness

**1. [Config key] Use slow-timeout instead of run.tests.timeout**
- **Found during:** Task 2 (nextest.toml)
- **Issue:** Plan and 08-RESEARCH used `run.tests.timeout`; nextest configuration reference uses `slow-timeout` for the period/terminate-after object.
- **Fix:** Set `slow-timeout = { period = "60s", terminate-after = 3 }` in nextest.toml.
- **Files modified:** nextest.toml
- **Committed in:** a4ea6b4

**2. [CI action] Use tool name cargo-nextest in install-action**
- **Found during:** Task 1
- **Issue:** taiki-e/install-action TOOLS.md lists the tool as `cargo-nextest`; version pin as `cargo-nextest@0.9`.
- **Fix:** Set `tool: cargo-nextest@0.9` in both Test and Test (macOS) steps.
- **Files modified:** .github/workflows/ci.yml
- **Committed in:** c303999

---

**Total deviations:** 2 (config key correctness, CI tool name)
**Impact on plan:** Aligns with current nextest and install-action behavior; no scope creep.

## Issues Encountered

- Local `cargo install cargo-nextest` failed without `--locked` (nextest requires locked install from source); CI uses prebuilt binaries from install-action, so no change to plan.
- One Task 1 commit (c303999) included an extra file (`crates/neurohid-core/tests/pipeline_integration.rs`) that was already staged; that file is from a separate plan (08-02). No revert per project policy; 08-02 can reference or skip that file as needed.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- TEST-01 tooling in place: nextest is the Test job runner; nextest.toml provides retries and timeout.
- CI should be run on a branch/PR to confirm Test and Test (macOS) pass with nextest; local `cargo nextest run --workspace` requires `cargo install --locked cargo-nextest` for developers who want to use nextest locally.

## Self-Check: PASSED

- nextest.toml: present at repo root
- 08-01-SUMMARY.md: present in .planning/phases/08-thorough-testing/
- Commits c303999, a4ea6b4 present in git log

---
*Phase: 08-thorough-testing*
*Completed: 2026-02-21*
