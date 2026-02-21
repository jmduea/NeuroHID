# Phase 8: Thorough Testing - Research

**Researched:** 2026-02-21  
**Domain:** Deterministic testing, integration boundaries, CI gates, E2E, test policy documentation  
**Confidence:** HIGH (stack, patterns); MEDIUM (CI/coverage alignment details)

## Summary

Phase 8 must deliver deterministic test runs, integration tests at key boundaries (IPC Rust↔Python, device→signal→decoder→action pipeline, config load/save), CI gates that reflect reality, at least one valuable E2E path, and documented test tiers and isolation policy. The repo already has a solid base: Rust unit tests across crates, one integration test (`extension_outlet_e2e`), IPC compat matrix (Rust transport + Python control/bridge smoke), config roundtrip tests in `neurohid-storage`, and coverage gates (Rust 35%, Python 50%). Gaps are: no nextest (only `cargo test`), no formal test policy doc, no pipeline-level integration test (device→signal→decoder→action as one flow), no E2E for Hub discover→connect→stream or runtime profile→decoder→action, and no pytest-rerunfailures or documented isolation rules. Planning should add nextest and optional `nextest.toml`, add integration tests at the missing boundaries, add one E2E path (e.g. runtime + config + decoder path or IPC roundtrip with real service), align CI and docs on coverage thresholds and flakiness handling, and add a single doc (e.g. in `docs/`) that defines unit vs integration vs E2E and isolation policy.

**Primary recommendation:** Use cargo-nextest for Rust (with optional retries/timeouts in `nextest.toml`), keep cargo-llvm-cov; add pytest-rerunfailures only where flakiness is identified; add integration tests at IPC, pipeline, and config boundaries; add one E2E path; document test tiers and isolation in `docs/development-guide.md` or `docs/testing.md`; sync development-guide coverage numbers with `ci.yml` env.

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| TEST-01 | Developer gets deterministic test runs (no flakiness from async/concurrency or shared state) via test policy and tooling (e.g. nextest, condition-based waits, isolation) | Standard stack: nextest (retries, timeouts); patterns: ephemeral ports, temp dirs, condition-based wait (see neurohid-service `wait_for_runtime_start`); doc: test tiers + isolation policy. |
| TEST-02 | Developer has integration tests at key boundaries (IPC Rust↔Python, device→signal→decoder→action pipeline, config load/save) so interface mismatches are caught in CI | Current: IPC (neurohid-ipc transport tests, IPC compat matrix with Python test_control_client/test_bridge); config (neurohid-storage ConfigStore save/load roundtrip). Gap: pipeline integration test; optional: config-through-service. Research: where to add pipeline test (neurohid-core or neurohid). |
| TEST-03 | CI gates reflect reality (coverage and flakiness addressed) so passing CI means safe-to-merge for the scope exercised | Current: PYTHON_COVERAGE_MIN 50, RUST_COVERAGE_MIN 35; coverage jobs upload to Codecov. Sync docs (development-guide says 48/30); add retries only for identified flaky tests; avoid masking flakiness with broad reruns. |
| TEST-04 | Developer has at least one valuable E2E path (e.g. Hub discover→connect→stream or runtime profile→decoder→action) exercised in tests | Current: extension_outlet_e2e (load extension, create outlet). Add one of: (a) runtime profile load → decoder → action path, or (b) IPC E2E with real service + Python client snapshot/control, or (c) Hub discover→connect→stream if feasible without new E2E framework (REQUIREMENTS: no Playwright). |
| TEST-05 | Test tiers and isolation policy are documented so contributors know what is unit vs integration vs E2E and how to avoid flakiness | No current doc. Add section (e.g. docs/testing.md or docs/development-guide.md): tier definitions, isolation rules (ports, dirs, env, IPC), and flakiness avoidance (condition waits, no sleep-only, retries policy). |

</phase_requirements>

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| cargo-nextest | 0.9.x (latest stable) | Rust test runner | Parallel per-test, retries, timeouts, JUnit XML; fits existing workspace; recommended in project research (STACK.md). |
| cargo-llvm-cov | (existing in CI) | Rust coverage | Keep; CI already uses it; use with nextest (e.g. build with coverage, run tests via nextest). |
| pytest | ≥9.0.2 (existing) | Python test runner | Already in pyproject.toml and CI. |
| pytest-cov | ≥7.0.0 (existing) | Python coverage | Already in CI with --cov-fail-under. |
| pytest-asyncio | ≥1.3.0 (existing) | Async Python tests | test_bridge uses IsolatedAsyncioTestCase; keep. |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| pytest-rerunfailures | ≥14.0 | Flaky test retries | Only for identified flaky tests (mark or CLI); avoid mixing with pytest-xdist for flaky suites. |
| nextest.toml | (repo root or .config) | Retries, timeouts | Optional; use for retries = 2 and run.tests.timeout for integration tests if needed. |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| cargo test | cargo nextest run | nextest: faster, retries, timeouts; keep cargo test for local one-off. |
| Broad pytest reruns | Per-test fix or @pytest.mark.flaky | Reruns mask root cause; prefer fix + minimal reruns. |
| New E2E framework (Playwright) | Existing Rust integration + binary tests | REQUIREMENTS out-of-scope; use in-repo integration/E2E only. |

**Installation (additions):**

- Rust: `cargo install cargo-nextest` or use CI action (e.g. taiki-e/install-action for nextest).
- Python: add `pytest-rerunfailures>=14.0` to `[project.optional-dependencies]` dev in `python/pyproject.toml`; no change to `uv run --project python pytest ...`.

## Architecture Patterns

### Recommended Test Layout (existing + extensions)

- **Rust:** Unit tests in `mod tests` per crate; integration tests as `crates/<crate>/tests/*.rs` (e.g. `neurohid-core/tests/extension_outlet_e2e.rs`). Add integration tests under `neurohid-core/tests/` or `neurohid/tests/` for pipeline and service-level flows.
- **Python:** `python/tests/` with unittest and pytest; IPC smoke: `test_control_client.py`, `test_bridge.py` (already in IPC compat matrix).
- **CI:** Keep impact-based routing (`classify-impact.ps1`); Test job runs `cargo test --workspace` (then switch to `cargo nextest run --workspace`) and extension_outlet_e2e; IPC compat matrix runs Rust transport tests + Python control/bridge; coverage jobs unchanged except optional nextest-aware invocation.

### Pattern 1: Condition-based wait (no sleep-only)

**What:** Poll until a condition holds, with a bounded deadline.  
**When:** Integration tests that depend on runtime/process state (e.g. "runtime ready", "port listening").  
**Example (from neurohid-service):**

```rust
async fn wait_for_runtime_start(runtime: &RuntimeHandle) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    loop {
        if runtime.snapshot().running {
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "runtime did not become active in time");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
```

### Pattern 2: Ephemeral resources

**What:** Unique temp dirs, ephemeral ports per test to avoid cross-test interference.  
**When:** Any integration test that uses filesystem or network.  
**Example:** `extension_outlet_e2e` uses `env::temp_dir().join("neurohid_ext_e2e")`; neurohid-ipc tests use `allocate_test_port()` (bind to 127.0.0.1:0).

### Pattern 3: Config roundtrip (existing)

**What:** Save default or modified config, load, assert equality.  
**When:** Config load/save boundary.  
**Example (neurohid-storage):** `ConfigStore::save` then `ConfigStore::load` in temp dir; assert format_version and key fields.

### Anti-Patterns to Avoid

- **Sleep-only waits:** Use condition-based wait with deadline instead.
- **Shared ports or temp paths across tests:** Use ephemeral port and unique temp dir per test.
- **Broad pytest reruns by default:** Fix root cause; use pytest-rerunfailures only for identified flaky tests.
- **E2E framework (e.g. Playwright) for v1.1:** Out of scope; use existing Rust integration and binary tests.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Parallel Rust test runs with retries | Custom runner script | cargo-nextest | nextest handles parallelism, retries, timeouts, JUnit output. |
| Flaky Python test reruns | Custom retry loop | pytest-rerunfailures | Mark or CLI; well maintained. |
| Coverage from nextest | Custom coverage glue | cargo llvm-cov with nextest (or existing CI flow) | Official docs; nextest can run under llvm-cov or run tests separately after coverage build. |

**Key insight:** Test runners and coverage tooling are standard; hand-rolled retries or parallelization increase maintenance and miss edge cases.

## Common Pitfalls

### Pitfall 1: Rust async/concurrency flakiness

**What goes wrong:** Tests fail intermittently due to ordering or timeouts.  
**Why it happens:** Sleep-based waits, shared state, or no timeout.  
**How to avoid:** Condition-based wait with deadline; isolate resources (ephemeral port, temp dir); use nextest timeouts for integration tests.  
**Warning signs:** Same test fails only on CI or under load.

### Pitfall 2: Python non-determinism / shared state

**What goes wrong:** ML or IPC tests vary by run (e.g. hash order, unseeded RNG).  
**Why it happens:** Shared env, global state, or no seed.  
**How to avoid:** Fix random seeds (and PYTHONHASHSEED) where relevant; per-test or per-suite isolation; avoid shared long-lived IPC connection across tests.  
**Warning signs:** test_trainer or test_decoder_and_errp flake; different order of test execution changes outcome.

### Pitfall 3: CI coverage gate vs doc mismatch

**What goes wrong:** development-guide says 48% Python / 30% Rust but ci.yml uses 50 / 35.  
**Why it happens:** Env vars updated in CI without doc update.  
**How to avoid:** Single source of truth (e.g. ci.yml env) and update development-guide to match; or document "see ci.yml for current gates."  
**Warning signs:** Contributors follow doc and are surprised by CI failure.

### Pitfall 4: Masking flakiness with retries

**What goes wrong:** CI always passes but tests are flaky; regressions slip.  
**Why it happens:** Broad reruns (e.g. --reruns 5 for all tests).  
**How to avoid:** Prefer fixing root cause; use retries only for known-flaky tests (nextest profile or pytest mark); document which tests are retried and why.  
**Warning signs:** Retry count high; same test appears in failure logs then passes on retry.

## Code Examples

### Rust: Ephemeral port (neurohid-ipc / neurohid-service)

```rust
fn allocate_test_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("ephemeral bind should succeed")
        .local_addr()
        .expect("socket address should resolve")
        .port()
}
```

### Rust: Config load/save roundtrip (neurohid-storage)

```rust
#[tokio::test]
async fn save_then_load_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let store = make_store(tmp.path().to_path_buf());
    let mut config = SystemConfig::default();
    config.signal.notch_filter_hz = 50.0;
    store.save(&config).await.unwrap();
    let loaded = store.load().await.unwrap();
    assert_eq!(loaded.signal.notch_filter_hz, 50.0);
}
```

### Python: IPC surface (test_bridge – protocol version)

```python
async def test_unsupported_version_emits_protocol_error(self) -> None:
    client = _FakeBridgeClient()
    session = _bridge.BridgeSession(client)
    should_stop = await session.handle_runtime_message({
        "v": 1, "channel": "trainer.stream", "msg_type": "hello", "seq": 1, "payload": {},
    })
    self.assertFalse(should_stop)
    self.assertEqual(client.sent[-1]["kind"], "error")
    self.assertEqual(client.sent[-1]["payload"]["code"], "unsupported_version")
```

### nextest.toml (optional)

```toml
[profile.default]
retries = { backoff = "fixed", count = 2 }
run.tests.timeout = { period = "60s", terminate-after = 3 }
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| cargo test only | cargo nextest (add in Phase 8) | v1.1 | Faster, retries, timeouts. |
| No test policy doc | Test tiers + isolation doc | Phase 8 | Contributors know unit vs integration vs E2E and how to avoid flakiness. |
| Only extension E2E | + one E2E path (runtime/profile or IPC) | Phase 8 | TEST-04 satisfied. |

**Deprecated/outdated:** None for this phase. Keep cargo test available for local use; nextest is additive.

## Open Questions

1. **Pipeline integration test placement**  
   - What we know: device→signal→decoder→action is in neurohid-core / neurohid; unit tests exist in neurohid-signal, neurohid-core/tasks (decoder, signal).  
   - What's unclear: Whether to add a single integration test in neurohid-core (e.g. `tests/pipeline_integration.rs`) that wires mock device → signal → decoder → action, or in neurohid with a minimal service.  
   - Recommendation: Prefer neurohid-core test with mock device and in-memory pipeline to avoid full binary; document as integration boundary test.

2. **E2E path choice (TEST-04)**  
   - What we know: extension_outlet_e2e exists; IPC compat matrix runs Rust + Python but not "real service + Python client" in one job; Hub discover→connect→stream would need GUI/headless strategy.  
   - What's unclear: Which single E2E delivers most value: (a) runtime + config load + decoder + action in one test, (b) spawn neurohid-service + Python client snapshot/control, or (c) something else.  
   - Recommendation: Option (b) is a strong candidate (already have service + control client; add one test that starts service, connects Python client, requests snapshot, optional control). Option (a) is good if pipeline integration test is extended to "full in-process pipeline" and considered E2E.

3. **nextest + cargo-llvm-cov in CI**  
   - What we know: CI runs `cargo llvm-cov --workspace ... --fail-under-lines`; nextest can run tests.  
   - What's unclear: Exact invocation (e.g. `cargo llvm-cov run --no-run` then `cargo nextest run` with coverage, or keep current and only add nextest for Test job without coverage).  
   - Recommendation: Keep rust-coverage job as-is initially; switch Test job to `cargo nextest run --workspace`; if coverage must use nextest, follow nextest docs for "run under coverage" (may require nextest as subprocess of llvm-cov or vice versa). Verify with nextest 0.9.x and cargo-llvm-cov before locking.

## Sources

### Primary (HIGH confidence)

- Project `.planning/research/STACK.md` — nextest, pytest-rerunfailures, testing stack.
- Project `.planning/research/SUMMARY.md` — phase order, pitfalls, test deliverables.
- Project `docs/integration-architecture.md`, `docs/protocol-and-api.md` — boundaries and IPC.
- Project `crates/neurohid-storage/src/config.rs`, `crates/neurohid-core/tests/extension_outlet_e2e.rs`, `crates/neurohid/src/bin/neurohid-service.rs` (tests) — patterns in repo.
- Project `.github/workflows/ci.yml` — current CI gates and coverage env.

### Secondary (MEDIUM confidence)

- Project `docs/development-guide.md` — test commands and coverage numbers (note: doc says 48/30, ci.yml 50/35).
- nextest: nexte.st docs (running, configuration) — retries, timeouts; coverage integration not verified at exact URL.

### Tertiary (LOW confidence)

- cargo-llvm-cov + nextest: exact CI recipe not re-verified on official sites; project STACK.md states compatibility.

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH — project research and repo usage.
- Architecture: HIGH — existing patterns and crate layout.
- Pitfalls: HIGH — from project PITFALLS/SUMMARY and repo inspection.
- nextest + llvm-cov CI recipe: MEDIUM — STACK says compatible; exact steps not re-fetched from official docs.

**Research date:** 2026-02-21  
**Valid until:** ~30 days (stable tooling).
