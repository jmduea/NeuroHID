# Pitfalls Research: Adding Testing, BrainFlow, and Framework–Hub Separation

**Domain:** Rust/Python biosignals stack (NeuroHID) — adding thorough testing, native BrainFlow integration, and framework-vs-Hub structural separation  
**Researched:** 2026-02-21  
**Confidence:** MEDIUM (codebase + official BrainFlow/docs + Rust/Python testing literature; some integration pitfalls inferred from patterns)

## Critical Pitfalls

### Pitfall 1: Test flakiness from async and concurrency (Rust)

**What goes wrong:**  
Rust integration and E2E tests fail intermittently due to race conditions, timeouts, or task ordering. Research on Rust flaky tests shows **asynchronous wait issues** (~34%) and **concurrency problems** (~25%) as dominant causes.

**Why it happens:**  
Tests spawn real tasks (DeviceTask, SignalTask, IpcTask, etc.) or use IPC with timeouts; slight timing or load variance causes passes/failures. Extension E2E already depends on build order and temp dirs; adding more concurrent or IPC-heavy tests multiplies nondeterminism.

**How to avoid:**  
- Use explicit wait oracles (e.g., poll until condition with bounded timeout) instead of bare `sleep`.  
- Isolate shared resources per test (e.g., unique temp dirs, unique IPC endpoints or ports).  
- Prefer unit tests with injected mocks for device/signal/IPC where possible; reserve E2E for fewer, well-scoped flows.  
- Document and centralize skip conditions (e.g., “example outlet not built”) so CI doesn’t treat skip as pass-by-accident.

**Warning signs:**  
- Same test fails only on one platform or in CI.  
- Tests that pass with `--test-threads=1` but fail with parallel runs.  
- Reliance on fixed `sleep(Duration::from_secs(...))` in tests.

**Phase to address:**  
Thorough testing phase (v1.1): define test tiers (unit / integration / E2E), add retry or timeout policy for known-flaky E2E only where acceptable, and add a checklist for new integration tests (resource isolation, no shared global state).

---

### Pitfall 2: Python test flakiness from non-determinism and shared state

**What goes wrong:**  
Python tests (bridge, decoder, trainer, IPC client) fail intermittently due to non-deterministic training, test order dependence, or shared env/process state.

**Why it happens:**  
ML/training code is inherently non-deterministic unless seeds and env are fixed. Integration tests that hit the real IPC or bridge can be order-dependent or leave state that affects the next test.

**How to avoid:**  
- Fix random seeds (and optionally `PYTHONHASHSEED`) in tests that run training or inference.  
- Run integration tests in isolated processes or with fresh `uv run` invocations where needed.  
- Avoid sharing a single long-lived IPC connection or server across tests; use per-test or per-suite setup/teardown.  
- Keep coverage gates but avoid making coverage the only gate for flaky tests.

**Warning signs:**  
- `pytest -x` passes but full suite sometimes fails.  
- Failures disappear when running a single test file.  
- Coverage drops or spikes randomly in CI.

**Phase to address:**  
Thorough testing phase: document Python test isolation policy, add pytest fixtures for IPC/bridge isolation, and add a small “determinism” check (e.g., two runs of same trainer test yield same metrics when seeded).

---

### Pitfall 3: BrainFlow “simulation” vs real SDK confusion

**What goes wrong:**  
The current BrainFlow backend in `neurohid-device` is a **simulation adapter** (mock device behind BrainFlow-style config and board catalogue); it does **not** link the real BrainFlow SDK. Code or docs that assume real hardware or real BoardShim behavior break when switching to native BrainFlow, or vice versa.

**Why it happens:**  
First-class BrainFlow work includes “docs, examples, Hub UX” and then “deeper integration (board config, streaming).” It’s easy to document or test against the simulation as if it were the SDK, then hit API/lifecycle mismatches when wiring the real SDK.

**How to avoid:**  
- Clearly label in code and docs: “BrainFlow simulation” (current) vs “BrainFlow native” (future).  
- When adding native SDK: introduce a feature or backend variant so simulation remains the default for no-SDK builds and CI.  
- Base native integration on official BrainFlow lifecycle: `prepare_session` → `start_stream` → `get_board_data` (or streaming API) → `stop_stream`; map NeuroHID `Device`/`Sample` to BrainFlow row layout (timestamp channel, marker channel, eeg_channels from board metadata).

**Warning signs:**  
- Tests or docs that assume “BrainFlow” means real hardware or real SDK without a feature flag.  
- Hub discovery/connection UX that only works with the current simulation and has no path for real board IDs/params.

**Phase to address:**  
Native BrainFlow phase (first-class then deeper): in the first phase, document simulation vs native and add examples that work with both; in the deeper phase, implement real SDK behind the same `Device`/`DeviceProvider` surface and validate board metadata (e.g., `brainflow_boards.cpp`-style fields) and row layout in tests.

---

### Pitfall 4: BrainFlow API and build mismatches (when adding real SDK)

**What goes wrong:**  
Rust bindings for BrainFlow require building the C++ core first; board IDs and metadata live in C constants and must be kept in sync across bindings. Mismatches in board ID enum, row layout (timestamp/marker/eeg_channels), or build order cause runtime errors or wrong data mapping.

**Why it happens:**  
BrainFlow’s Rust binding relies on generated code; adding new boards requires updating `brainflow_constants.h`, the board controller, and “all bindings” (including Rust, e.g. via `cargo build --features generate_binding`). Row layout is board-specific and defined in `brainflow_boards.cpp`.

**How to avoid:**  
- Pin and document a BrainFlow version for the native backend; document build steps (build C++ core, then Rust) in development-guide and CI if native is enabled.  
- Map BrainFlow’s 2D row layout explicitly to NeuroHID `Sample` (and any channel metadata); don’t assume a single layout for all boards.  
- Use BrainFlow’s synthetic board (or playback) in CI/tests where possible so native tests don’t require hardware.

**Warning signs:**  
- Build failures on CI with “BrainFlow not found” or missing symbols.  
- Runtime errors when reading samples (wrong row index, wrong channel count).  
- Hub or runtime showing wrong channel names or counts for a board.

**Phase to address:**  
Deeper BrainFlow integration phase: add a “native BrainFlow” build/CI path (optional job or feature), document board metadata and row mapping in `neurohid-device`, and add at least one integration test using synthetic board.

---

### Pitfall 5: Framework–Hub separation breaks Hub or runtime

**What goes wrong:**  
While moving to a clear “framework” (what devs depend on) vs “Hub” (one app on top), dependency or feature changes break the Hub GUI or the headless runtime: missing symbols, broken re-exports, or Hub depending on crates it should not (e.g. device/signal directly instead of via core).

**Why it happens:**  
Hub currently depends on `neurohid-core`, `neurohid-ipc`, `neurohid-storage`, `neurohid-calibration`; core exposes a `facade` for IPC and storage. Moving types or removing re-exports, or changing which crates the “framework” includes, can break Hub or the main binary if they still rely on the old structure. Rust’s public API is sensitive: even adding new public items can break downstream that use `use crate::*`.

**How to avoid:**  
- Define the “framework” surface in one place (e.g. `neurohid-core` + `neurohid-sdk` re-exports) and document it; Hub must depend only on that surface (and calibration if needed), not on component crates directly.  
- When moving code: do dependency changes and facade updates in one coherent change set; run full workspace tests and Hub/service/validate binaries after.  
- Consider `cargo-public-api` or similar to track intentional API surface and catch accidental breaking changes.

**Warning signs:**  
- Hub or `neurohid-service` fails to build after a “framework” refactor.  
- New dep in Hub on `neurohid-device` or `neurohid-signal` (reverses existing boundary).  
- Doc says “use core::facade” but code still imports from `neurohid_ipc` or `neurohid_storage` in Hub.

**Phase to address:**  
Framework vs Hub separation phase: enforce and document “Hub depends only on core (and calibration); core re-exports what Hub needs”; add a boundary check (script or CI) that Hub’s Cargo.toml does not depend on component crates other than those allowed.

---

### Pitfall 6: E2E and integration tests that assume runtime/Hub process layout

**What goes wrong:**  
Integration or E2E tests start the real service or Hub, or rely on a specific process/port layout; when framework separation or startup order changes, tests become flaky or start failing (e.g. port in use, timeout waiting for “ready”).

**Why it happens:**  
Tests were written against a single binary or a fixed startup sequence; after splitting or changing how the runtime is built or how the Hub connects, the test’s assumptions no longer hold.

**How to avoid:**  
- Prefer in-process or in-memory integration tests where possible (e.g. `RuntimeBuilder` + `NeuroHidService` in test, no separate process).  
- If tests start a real process, use unique temp dirs and ephemeral ports (or a single well-known test port with mutex/serialization).  
- Document “contract” for “runtime ready” (e.g. control RPC responds) and wait for that instead of a fixed delay.  
- Keep E2E tests minimal and focused (e.g. extension outlet load, one control round-trip); don’t grow them into full workflow tests without explicit isolation.

**Warning signs:**  
- “Address already in use” or “connection refused” in CI.  
- Tests that `sleep` then assume runtime is up.  
- E2E that only passes when run after a specific other job or in a specific order.

**Phase to address:**  
Thorough testing phase: document integration/E2E test policy (in-process vs subprocess, ports, timeouts), and add a single “runtime ready” helper used by all tests that start the service.

---

### Pitfall 7: IPC contract drift between Rust and Python

**What goes wrong:**  
Rust runtime and Python bridge share an IPC protocol (envelope, channels, message shapes). Changes to one side (e.g. new field, new channel, or different serialization) break the other; tests pass in isolation but fail when both run together, or in production.

**Why it happens:**  
Protocol and API reference live in docs and possibly in shared types; implementation on both sides can drift if changes are made without updating the other side and the compatibility matrix.

**How to avoid:**  
- Treat IPC as a versioned contract: document in `protocol-and-api.md` (or equivalent) and run the existing “IPC compat matrix” CI (Rust transport + Python client tests) on every protocol-touching change.  
- Prefer shared schema or generated types where feasible (e.g. one source of truth for message shapes).  
- When adding BrainFlow or new runtime features that affect IPC, extend the contract and update both Rust and Python in the same milestone.

**Warning signs:**  
- Python tests pass but “unified service multiplexing smoke” or “Python IPC surface smoke” fails.  
- New Rust event types or control RPCs that Python never handles (or vice versa).  
- Protocol docs out of date with code.

**Phase to address:**  
All three feature areas: any change that touches IPC (testing with IPC, BrainFlow events, or framework re-exports of IPC types) must trigger protocol verification; keep “protocol” in the impact classifier so CI runs protocol contracts when needed.

---

## Moderate Pitfalls

### Pitfall 8: Coverage gates hiding flakiness

**What goes wrong:**  
Rust or Python coverage gates (e.g. `RUST_COVERAGE_MIN`, `PYTHON_COVERAGE_MIN`) pass because flaky tests are retried or skipped, so coverage looks good while real failures are masked.

**Prevention:**  
Don’t retry flaky tests by default in CI; fix or quarantine them. Use coverage on stable tests; if a test is skipped (e.g. extension not built), ensure the gate explicitly allows that skip and doesn’t count it as covered.

---

### Pitfall 9: Framework surface re-export bloat

**What goes wrong:**  
The “framework” (core + SDK) re-exports too many types from component crates, so that adding or changing a component forces a major version or breaks embedders. Effective Rust recommends not exposing dependency types in your API when avoidable.

**Prevention:**  
Re-export only what embedders (Hub, external users) need. Prefer core-owned types and adapters at the boundary; document the stable facade and use `cargo-public-api` to track it. When adding BrainFlow or new device types, expose them through the existing Device/Provider abstraction, not raw BrainFlow types.

---

### Pitfall 10: Hub UX parity for BrainFlow vs LSL/Mock

**What goes wrong:**  
Hub discovery/connection UX works well for LSL or Mock but BrainFlow (simulation or native) is a second-class path: wrong labels, missing board list, or no way to set board-specific params (port, MAC, etc.).

**Prevention:**  
In first-class BrainFlow phase, treat BrainFlow as a peer backend in the UI: same discovery/connection flow, backend selector, and config for board ID and params. Use the same normalized device metadata (e.g. `DeviceId`, `source_id`) so the rest of the pipeline stays backend-agnostic.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Skip E2E when extension not built | Fast CI when example not built | Easy to forget to build; coverage gap | Only with explicit skip reason and doc; prefer CI to build example first |
| Add `sleep` in integration test to “wait for ready” | Test passes locally | Flaky under load; slows CI | Never as permanent solution; use condition + timeout |
| Hub depends on neurohid-ipc directly | Fewer indirections | Breaks framework boundary; harder to split later | No; use core::facade |
| Document “BrainFlow” without simulation vs native | Simpler docs | Confusion when adding real SDK | No; clarify from first-class phase |
| New public re-export in core to unblock Hub | Quick fix | Expands API surface; breakage risk for embedders | Only if documented as part of framework surface and reviewed |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|-------------------|
| BrainFlow (native) | Assume current simulation = real SDK; ignore prepare_session/start_stream lifecycle | Treat simulation as mock; map BoardShim lifecycle and row layout to Device/Sample; use synthetic board in CI |
| BrainFlow (Rust build) | Expect crate to build without C++ core | Document and automate: build BrainFlow C++ then Rust; optional CI job or feature |
| IPC (Rust ↔ Python) | Change envelope or channel on one side only | Update protocol doc and both implementations; run IPC compat matrix CI |
| Hub ↔ Runtime | Add Hub dependency on device/signal/core internals | Hub uses only core (and calibration); runtime access via RuntimeHandle and core::facade |
| Extension E2E | Rely on global target dir or one-off build | CI builds neurohid-outlet-example first; test uses CARGO_MANIFEST_DIR/target to find dylib; unique temp dir per run |

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|----------------|------------|
| Thorough testing | Flakiness from async/concurrency and IPC timing | Isolate resources; explicit wait oracles; document test tiers and retry policy |
| Thorough testing | Python non-determinism and shared state | Fix seeds; per-test IPC/env isolation; determinism check for trainer tests |
| Native BrainFlow (first-class) | Simulation vs native confusion in docs/UX | Label “simulation” vs “native”; same UX path for BrainFlow as other backends |
| Native BrainFlow (deeper) | Board ID/row layout/build mismatches | Pin BrainFlow version; document build; map rows explicitly; synthetic board in CI |
| Framework vs Hub | Breaking Hub or runtime with dep/facade changes | Single coherent refactor; Hub only on core (+ calibration); boundary check in CI |
| Any (IPC touch) | Protocol drift | Run protocol impact and IPC compat matrix; update both Rust and Python |

---

## "Looks Done But Isn't" Checklist

- [ ] **BrainFlow “first-class”:** Often missing clear simulation vs native story — verify docs and Hub UX distinguish them and that “deeper” has a build path.
- [ ] **Testing “thorough”:** Often missing isolation and determinism — verify no shared global state, seeds fixed in Python ML tests, and E2E use “runtime ready” instead of sleep.
- [ ] **Framework separation:** Often missing enforcement — verify Hub Cargo.toml has no disallowed deps and that facade is the single documented way for Hub to get IPC/storage.
- [ ] **E2E extension test:** Often missing “build first” in CI — verify extension outlet is built before `extension_outlet_e2e` and that skip is explicit when lib missing.
- [ ] **IPC contract:** Often missing dual-side update — verify protocol doc and both Rust and Python implementations updated together when adding/changing messages.

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Rust test flakiness (async/concurrency) | Thorough testing | Integration tests run with parallelism; no sleep-only waits; checklist for new tests |
| Python test flakiness (non-determinism/shared state) | Thorough testing | pytest isolation documented; seeded trainer test; no order-dependent failures |
| BrainFlow simulation vs SDK confusion | Native BrainFlow (first-class + deeper) | Docs and code paths clearly separate simulation and native; examples run with both |
| BrainFlow API/build mismatches | Native BrainFlow (deeper) | Optional native CI job; row mapping doc; synthetic board test |
| Framework–Hub breakage | Framework vs Hub separation | Hub builds with only core (+ calibration); boundary script/CI passes |
| E2E assumptions (process/ports) | Thorough testing | E2E policy doc; in-process preferred; “runtime ready” helper used |
| IPC contract drift | All (when touching IPC) | Protocol impact triggers compat matrix; protocol doc updated |

---

## Sources

- BrainFlow: [BrainFlow Dev](https://brainflow.readthedocs.io/en/stable/BrainFlowDev.html) (add new boards, Rust bindings, emulator), [Adding new boards](https://brainflow.org/2022-11-01-adding-new-boards/) (board IDs, bindings, metadata).
- Rust flaky tests: “A Preliminary Study of Fixed Flaky Tests in Rust Projects on GitHub” (async ~34%, concurrency ~25%).
- Python flakiness: Trunk.io / pytest guidance (concurrency, order, external deps, ML non-determinism).
- Rust API stability: Effective Rust (re-exports, avoid exposing dependency types); Stack Overflow / predr.ag (breaking changes, glob imports).
- NeuroHID: `.planning/PROJECT.md`, `docs/crate-boundaries.md`, `docs/integration-architecture.md`, `crates/neurohid-core/src/lib.rs` (facade), `crates/neurohid-device/src/brainflow.rs` (simulation adapter), `crates/neurohid-core/tests/extension_outlet_e2e.rs`, CI workflows (IPC compat matrix, extension E2E).

---
*Pitfalls research for: adding testing, BrainFlow integration, and framework–Hub separation to NeuroHID*  
*Researched: 2026-02-21*
