---
phase: 08-thorough-testing
plan: 02
subsystem: testing
tags: [integration-test, pipeline, neurohid-core, ci, ipc, config]

# Dependency graph
requires: []
provides:
  - Pipeline integration test (device→signal→decoder→action) in neurohid-core
  - CI explicitly runs pipeline_integration; IPC and config boundaries confirmed
affects: [08-03, 08-04]

# Tech tracking
tech-stack:
  added: []
  patterns: [mock device + in-memory pipeline, condition-based wait with deadline]

key-files:
  created: [crates/neurohid-core/tests/pipeline_integration.rs]
  modified: [.github/workflows/ci.yml]

key-decisions:
  - "Pipeline integration test lives in neurohid-core with mock device and in-memory pipeline (no full binary)"
  - "IPC and config boundaries covered by existing jobs; no CI job changes beyond explicit pipeline_integration step"

patterns-established:
  - "Integration test: mock samples → SignalPipeline → features → create_decoder (fallback) → assert action received"

requirements-completed: [TEST-02]

# Metrics
duration: 15min
completed: 2026-02-21
---

# Phase 08 Plan 02: Pipeline and Boundary Integration Tests Summary

**Pipeline integration test in neurohid-core (device→signal→decoder→action) and IPC/config boundaries confirmed in CI.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 2 completed
- **Files modified:** 2 (pipeline_integration.rs present from 08-01; ci.yml updated)

## Accomplishments

- Pipeline boundary exercised in one integration test: mock device (in-memory samples) → SignalPipeline → FeatureVector → DecoderTask (fallback) → Action received.
- CI Test and Test (macOS) jobs run `pipeline_integration` explicitly (with `extension_outlet_e2e`).
- IPC and config integration tests confirmed: `cargo nextest run --workspace` runs neurohid-ipc and neurohid-storage tests; ipc-compat-matrix runs Rust transport smoke (tcp + local_socket), unified service multiplexing smoke, and Python test_control_client + test_bridge.

## Task Commits

1. **Task 1: Add pipeline integration test** — Pipeline test artifact was already present in repo (commit `c303999` from 08-01). Verified test passes and is included in workspace/CI.
2. **Task 2: Confirm IPC and config in CI** — `cf7455d` (chore): add explicit "Pipeline integration" step to Test jobs; document that IPC and config boundaries are covered by existing jobs.

## Files Created/Modified

- `crates/neurohid-core/tests/pipeline_integration.rs` — Integration test: mock samples, SignalPipeline, create_decoder with fallback, condition-based wait for ≥1 action.
- `.github/workflows/ci.yml` — Added "Pipeline integration" step to Test and Test (macOS) (cargo nextest run -p neurohid-core --test pipeline_integration).

## Decisions Made

- No new CI job for neurohid-storage: config roundtrip tests run as part of `cargo nextest run --workspace` in the Test job.
- Explicit pipeline_integration step added for visibility and parity with extension_outlet_e2e.

## Deviations from Plan

None — plan executed as written. Task 1 artifact already existed; Task 2 confirmed boundaries and added explicit CI step.

## Self-Check: PASSED

- 08-02-SUMMARY.md present; pipeline_integration.rs exists and test passes
- CI runs pipeline_integration (explicit step), neurohid-ipc (workspace + ipc-compat-matrix), neurohid-storage (workspace)
- Commits c303999, cf7455d present in repo

---
*Phase: 08-thorough-testing*
*Plan: 02*
*Completed: 2026-02-21*
