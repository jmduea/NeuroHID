# Testing: Tiers, Isolation, and Flakiness

This document is the single source for test tier definitions, isolation policy, and how to avoid flakiness in the NeuroHID repository. See [Development Guide](development-guide.md) for build and run commands.

## Test Tiers

### Unit

- **Scope:** A single crate or module; no separate process, no network.
- **Purpose:** Fast feedback on logic, pure functions, and in-process behavior.
- **Examples:** `mod tests` in a crate; tests that exercise one function or type in isolation with mocks or in-memory state.
- **Run:** Same process as the code under test; no IPC, no config roundtrip, no real device.

### Integration

- **Scope:** Cross-crate or cross-process; may use IPC, config load/save roundtrip, or multiple components together.
- **Purpose:** Catch interface mismatches and boundary behavior (in-process Python↔Rust bindings, external IPC clients, device→signal→decoder→action pipeline, config persistence).
- **Examples:** Python control client / bridge tests using mock `RuntimeHandle`; IPC transport tests for external clients; `ConfigStore` save-then-load roundtrip; pipeline or service-level flows.
- **Run:** May spawn processes or use temp dirs and ephemeral ports; must follow isolation policy below.

### E2E (End-to-end)

- **Scope:** Full binary plus client or multi-process flow (e.g. runtime + config + decoder path, or service + Python client).
- **Purpose:** Validate a complete path a user or automation would take.
- **Examples:** Extension outlet E2E (load extension, create outlet); runtime profile load → decoder → action; service started with real IPC + Python client snapshot/control.
- **Run:** Real binaries and optional real config; still use ephemeral ports and unique temp dirs.

## Isolation Policy

To avoid cross-test interference and ordering-dependent failures:

| Resource   | Policy |
|-----------|--------|
| **Ports** | Use ephemeral ports (e.g. bind `127.0.0.1:0` and use the assigned port). Do not hardcode ports or share one port across tests. |
| **Dirs**  | Use a unique temp dir per test (e.g. `tempfile::tempdir()` or `env::temp_dir().join("neurohid_…")` with a unique suffix). Do not share a single temp path across tests. |
| **Env**   | Do not rely on shared environment variables that affect test order or outcome. Prefer explicit per-test config. |
| **IPC**   | One connection per test or explicit cleanup so connections do not leak or affect other tests. |

**In-repo references:**

- Ephemeral port: `neurohid-ipc` and `neurohid-service` tests use a helper that binds to `127.0.0.1:0` and reads `local_addr().port()` (e.g. `allocate_test_port()`).
- Temp dir + config roundtrip: `neurohid-storage` uses a temp dir and `ConfigStore::save` then `ConfigStore::load` in tests (e.g. `save_then_load_roundtrip`).
- Condition-based readiness: `neurohid-service` tests use a bounded wait on runtime state (e.g. `wait_for_runtime_start`) instead of a fixed sleep.

## Flakiness Avoidance

### Condition-based wait (no sleep-only)

For integration or E2E tests that depend on process or network readiness:

- **Do:** Poll until a condition holds, with a **deadline**. For example: “runtime running” or “port listening” with a max wait (e.g. 3 seconds) and short poll interval (e.g. 25 ms).
- **Don’t:** Use only `sleep(Duration::from_secs(N))` without checking the condition; that is brittle and slows CI.

**In-repo reference:** `neurohid-service` tests use `wait_for_runtime_start` which loops until `runtime.snapshot().running` or the deadline is exceeded.

### Fix root cause before adding retries

- Prefer fixing the underlying cause of flakiness (ordering, shared state, missing synchronization) over turning on retries everywhere.
- Retries hide real bugs and can make CI unreliable.

### Retries only for identified flaky tests

- **Rust:** Use `nextest.toml` (e.g. retries, timeouts) for tests that are known to be flaky; document in this doc or in the plan which tests are retried and why.
- **Python:** Use `pytest-rerunfailures` only for specific tests or marks (e.g. `@pytest.mark.flaky`), not a global `--reruns 5` for the whole suite.
- Document which tests are retried and the reason (e.g. “external service occasionally slow”) so future contributors don’t assume all tests are stable.

## Summary

| Tier         | Scope                          | Isolation requirements        |
|-------------|---------------------------------|-------------------------------|
| Unit        | Single crate/module, no process | N/A (in-process only)         |
| Integration | Cross-crate, IPC, config, pipeline | Ephemeral ports, temp dirs, no shared env, one IPC connection per test or cleanup |
| E2E         | Full binary + client / multi-process | Same as integration          |

Follow isolation policy and condition-based waits so test runs stay deterministic and contributors know what counts as unit vs integration vs E2E and how to avoid flakiness.
