---
phase: 08-thorough-testing
verified: "2026-02-21T00:00:00Z"
status: passed
score: 10/10 must-haves verified
---

# Phase 08: Thorough Testing Verification Report

**Phase Goal:** Developer has confidence that tests are deterministic, key boundaries are covered, and CI reflects reality.

**Verified:** 2026-02-21  
**Status:** passed  
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #   | Truth | Status | Evidence |
| --- | ----- | ------ | -------- |
| 1   | Rust tests run via cargo-nextest with retries and timeouts in CI | ✓ VERIFIED | `ci.yml`: Install cargo-nextest@0.9, `cargo nextest run --workspace`; `nextest.toml`: `retries = { backoff = "fixed", count = 2 }`, `slow-timeout = { period = "60s", terminate-after = 3 }` |
| 2   | Local and CI test runs are deterministic (no sleep-only waits in new tests) | ✓ VERIFIED | `docs/testing.md` documents condition-based wait; `pipeline_integration.rs` uses `timeout(deadline, action_rx.recv())`; E2E uses `_wait_for_port` / `_wait_for_snapshot` with deadlines |
| 3   | Pipeline boundary (device→signal→decoder→action) is exercised in one integration test | ✓ VERIFIED | `crates/neurohid-core/tests/pipeline_integration.rs`: mock samples → SignalPipeline → features → create_decoder (fallback) → action channel; asserts ≥1 action received |
| 4   | IPC and config integration tests remain in CI and are stable | ✓ VERIFIED | Workspace includes neurohid-ipc, neurohid-storage; `cargo nextest run --workspace` runs all; `ci.yml` has explicit steps for extension_outlet_e2e and pipeline_integration; ipc-compat-matrix runs neurohid-ipc transport tests |
| 5   | Documentation coverage thresholds match CI (single source of truth) | ✓ VERIFIED | `docs/development-guide.md`: "Python coverage gate... currently 50", "Rust... currently 35", "Source of truth: .github/workflows/ci.yml (env)" |
| 6   | Passing CI means safe-to-merge for the scope exercised; flakiness not masked by broad retries | ✓ VERIFIED | development-guide: "CI passing means safe-to-merge for the scope exercised"; "broad reruns are avoided so flakiness is fixed at root cause" |
| 7   | At least one E2E path runs in CI: service started, Python client connects, snapshot or control exercised | ✓ VERIFIED | `ci.yml` job `e2e-service-client`: builds neurohid-service, runs `uv run --project python pytest python/tests/test_e2e_service_client.py`; test spawns service, connects, `snapshot()`, `set_output_enabled` |
| 8   | E2E uses existing Rust service and Python client (no new E2E framework) | ✓ VERIFIED | test uses subprocess + `NeuroHidControlClient`; no Playwright or new E2E framework |
| 9   | Contributors can read what is unit vs integration vs E2E and how to avoid flakiness | ✓ VERIFIED | `docs/testing.md`: Unit / Integration / E2E sections; Flakiness Avoidance (condition-based wait, fix root cause, retries only for identified flaky tests) |
| 10  | Isolation policy (ports, dirs, env, IPC) and retries policy are documented | ✓ VERIFIED | `docs/testing.md`: Isolation Policy table (ports, dirs, env, IPC); Retries only for identified flaky tests; in-repo references (allocate_test_port, save_then_load_roundtrip, wait_for_runtime_start) |

**Score:** 10/10 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `nextest.toml` | Retries and run timeout for integration tests | ✓ VERIFIED | Exists at repo root; contains `retries`, `slow-timeout`; ≥7 lines |
| `.github/workflows/ci.yml` | Test job invoking nextest | ✓ VERIFIED | Contains "nextest"; Test and Test (macOS) run `cargo nextest run --workspace` and nextest for extension_outlet_e2e, pipeline_integration |
| `crates/neurohid-core/tests/pipeline_integration.rs` | Pipeline integration test | ✓ VERIFIED | Exists; 115 lines (min 40); uses device→signal→decoder→action types and flow |
| `docs/development-guide.md` | Coverage gate numbers aligned with ci.yml | ✓ VERIFIED | Contains "50", "35"; states ci.yml env as source of truth |
| `python/tests/test_e2e_service_client.py` | E2E test: service + Python client | ✓ VERIFIED | Exists; 153 lines (min 30); spawn, connect, snapshot, control (set_output_enabled) |
| `docs/testing.md` | Test tiers and isolation policy | ✓ VERIFIED | Exists; contains "unit", "integration", "E2E"; isolation and retries policy |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| .github/workflows/ci.yml | nextest | Test and Test (macOS) job steps | ✓ WIRED | Steps run `cargo nextest run --workspace` and `cargo nextest run -p neurohid-core --test ...` |
| pipeline_integration.rs | neurohid-core pipeline types | mock device and in-memory pipeline | ✓ WIRED | Uses ServiceState, create_decoder, SignalPipeline, FeatureVector, mpsc for actions |
| docs/development-guide.md | .github/workflows/ci.yml | PYTHON_COVERAGE_MIN and RUST_COVERAGE_MIN | ✓ WIRED | Doc states 50/35 and "defined in .github/workflows/ci.yml (env)" |
| test_e2e_service_client.py | neurohid-service binary | spawn process, connect, snapshot/control | ✓ WIRED | Popen service with --control-port; NeuroHidControlClient; snapshot(), set_output_enabled() |
| docs/development-guide.md | docs/testing.md | link or See also | ✓ WIRED | "see [Test tiers and isolation](testing.md)" in Validation and Testing |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| TEST-01 | 08-01 | Deterministic test runs (nextest, policy, isolation) | ✓ SATISFIED | nextest.toml + ci.yml; docs/testing.md policy; pipeline/E2E use condition-based wait |
| TEST-02 | 08-02 | Integration tests at key boundaries (IPC, pipeline, config) | ✓ SATISFIED | pipeline_integration.rs; workspace runs neurohid-ipc, neurohid-storage; ipc-compat-matrix |
| TEST-03 | 08-03 | CI gates reflect reality (coverage, flakiness addressed) | ✓ SATISFIED | development-guide 50/35, source of truth ci.yml; retries/flakiness policy documented |
| TEST-04 | 08-04 | At least one valuable E2E path exercised in tests | ✓ SATISFIED | test_e2e_service_client.py; e2e-service-client job in ci.yml (Linux) |
| TEST-05 | 08-05 | Test tiers and isolation policy documented | ✓ SATISFIED | docs/testing.md; development-guide links to it |

No requirement IDs in plans are orphaned; REQUIREMENTS.md maps TEST-01–TEST-05 to Phase 8. All five are accounted for and satisfied.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| (none) | — | — | — | No TODO/FIXME/placeholder or stub implementations found in phase artifacts |

### Gaps Summary

None. All must-haves from plans 08-01 through 08-05 are present, substantive, and wired. Phase goal is achieved.

---

_Verified: 2026-02-21_  
_Verifier: Claude (gsd-verifier)_
