# Project Research Summary

**Project:** NeuroHID v1.1 — Testing, BrainFlow & Framework Separation  
**Domain:** Rust/Python biosignals (EEG) stack — BCI tooling with device pipeline, ML bridge, and IDE-like Hub  
**Researched:** 2026-02-21  
**Confidence:** HIGH (architecture, testing patterns); MEDIUM (BrainFlow Rust path, feature prioritization)

## Executive Summary

NeuroHID v1.1 is a confidence and clarity milestone: thorough testing, first-class BrainFlow integration (docs/examples and Hub UX, then deeper SDK/streaming), and a clear in-repo boundary between the reusable framework (what embedders depend on) and the Hub application. Experts build this by keeping the device pipeline behind stable traits, testing at boundaries (unit → integration → minimal E2E), and treating BrainFlow as one backend behind the existing DeviceProvider/Device abstraction—not a separate product surface.

The recommended approach is to do **framework–Hub separation first** (docs + dependency audit, no new crates), then **thorough testing** (nextest, flakiness reduction, integration tests at IPC and pipeline boundaries), then **BrainFlow** in two waves: first-class docs/examples and Hub discovery/connection UX, then deeper native SDK and streaming behind a feature flag with simulation retained for CI. Key risks are test flakiness from async/concurrency and from Python non-determinism, BrainFlow simulation-vs-native confusion and build/API mismatches when adding the real SDK, and framework separation breaking Hub if dependency or facade changes are done piecemeal. Mitigations: explicit wait oracles and resource isolation in tests; clear labeling of simulation vs native and a pinned BrainFlow build path; a single coherent refactor for the framework boundary with a CI check that Hub depends only on core (and calibration).

## Key Findings

### Recommended Stack

Stack research (STACK.md) adds only a few elements for v1.1; the existing Rust 2024/1.85+, Python 3.12+, uv, neurohid-* crates, eframe/egui, tract-onnx, and IPC stack is unchanged.

**Core additions:**
- **cargo-nextest** (0.9.x) — Rust test runner; faster parallel runs, built-in retries for flaky tests, JUnit XML and timeout controls; works with existing cargo-llvm-cov. Add as dev/CI tool with optional `nextest.toml`.
- **pytest-rerunfailures** (≥14.0) — Python flaky retries; use sparingly; avoid mixing with pytest-xdist for flaky suites. Optional/dev dep in `python/pyproject.toml`.
- **BrainFlow (Rust)** — No crates.io crate; use **git dependency** on `brainflow-dev/brainflow` (rust_package/brainflow) under a new feature (e.g. `brainflow-native`) in neurohid-device. Build requires BrainFlow C/C++ core first (e.g. `tools/build.py`), then Rust; document build order and env (e.g. BRAINFLOW_DIR). Keep mock/simulation path for default and CI.
- **Framework boundary** — No new frameworks or crates; layout and documentation only. Optionally document in `docs/crate-boundaries.md` (or `docs/framework-surface.md`) and consider a facade crate only if a single “framework” dependency name is needed later.

**What not to add:** Separate E2E framework (e.g. Playwright for Hub) for v1.1; cargo-tarpaulin (keep cargo-llvm-cov); third-party brainflow crate from crates.io; moving crates to another repo in v1.1.

### Expected Features

From FEATURES.md: v1.1 scope is three areas—thorough testing, native BrainFlow (first wave then deeper), and framework–Hub separation.

**Must have (table stakes):**
- Unit tests for new/changed code; deterministic tests (no flakiness); integration tests at boundaries (IPC, pipeline); CI gates that reflect reality.
- First-class BrainFlow docs and runnable examples (synthetic board); Hub discovery/connection UX for BrainFlow; device-agnostic API preserved (BrainFlow as one backend).
- Clear “what devs depend on” (framework surface); Hub as one application; documented boundary; no reverse coupling (components do not depend on Hub/GUI).

**Should have (competitive):**
- E2E only where valuable (e.g. one critical path); stable validation harness in CI (Soak/LatencyMatrix/BootMatrix) — deferred per PROJECT.md but aligned with testing.
- Board-specific config in Hub; unified streaming path BrainFlow → pipeline; synthetic/dummy board in CI.
- Framework as distinct crate set or layout; stable SDK feature matrix; single binary (neurohid, neurohid-service, neurohid-validate) still works.

**Defer (v2+):**
- Full framework repo split; validation harness in CI as a formal requirement; BrainFlow-only Hub (keep multi-backend).

### Architecture Approach

From ARCHITECTURE.md: existing layers (types → components → core → hub | sdk | binary) and data flow (DeviceTask → SignalTask → DecoderTask → ActionTask → OutletTask; Hub/CLI via RuntimeHandle and core facade; Python over IPC) are unchanged. v1.1 integrates by adding tests, extending neurohid-device BrainFlow path, and documenting/enforcing the framework–Hub boundary.

**Major components (v1.1-relevant):**
1. **neurohid-core** — Integration tests (e.g. IPC, extension outlet E2E); ownership for runtime/service integration tests; no new crates.
2. **neurohid-device** — BrainFlow: keep simulation adapter; add native SDK path behind same Device/DeviceProvider traits under feature `brainflow-native`; all BrainFlow logic stays here.
3. **neurohid-hub** — BrainFlow discovery/connection UX in Devices screen; optional Hub-specific integration tests; depends only on core (facade), not device/signal directly.
4. **neurohid-sdk** — Documented as framework entrypoint; re-exports and feature set define the embedder surface.
5. **Docs** — New or updated: framework vs Hub boundary, BrainFlow setup and Hub UX, build order for native BrainFlow.

### Critical Pitfalls

From PITFALLS.md — top five with prevention:

1. **Rust test flakiness (async/concurrency)** — Use explicit wait oracles (poll until condition, bounded timeout); isolate resources (unique temp dirs, ephemeral ports); prefer unit tests with mocks; reserve E2E for few, well-scoped flows. Phase: thorough testing; checklist for new integration tests.

2. **Python test flakiness (non-determinism/shared state)** — Fix random seeds (and PYTHONHASHSEED) in ML/training tests; per-test or per-suite IPC/env isolation; avoid shared long-lived IPC connection. Phase: thorough testing; document isolation policy; seeded trainer determinism check.

3. **BrainFlow simulation vs real SDK confusion** — Label clearly “BrainFlow simulation” vs “BrainFlow native” in code and docs; keep simulation default for no-SDK builds and CI; base native on official lifecycle (prepare_session → start_stream → get_board_data → stop_stream). Phase: first-class then deeper BrainFlow.

4. **BrainFlow API/build mismatches** — Pin and document BrainFlow version; document build steps (C++ core then Rust); map row layout explicitly to NeuroHID Sample; use synthetic/playback board in CI. Phase: deeper BrainFlow integration.

5. **Framework–Hub separation breaking Hub or runtime** — Define framework surface in one place (core + SDK re-exports); Hub depends only on that (and calibration); do dependency and facade changes in one coherent change set; add CI boundary check (Hub Cargo.toml no disallowed deps). Phase: framework vs Hub separation.

Additional high-impact: **E2E/integration tests assuming process/port layout** — Prefer in-process integration; unique temp dirs and ephemeral ports; “runtime ready” contract and wait for it instead of sleep. **IPC contract drift** — Versioned contract; update protocol doc and both Rust and Python when changing messages; run IPC compat matrix on protocol-touching changes.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Framework–Hub Separation (structural + docs)

**Rationale:** Defines the boundary that testing and BrainFlow live within; no code flow change; cheap (docs + audit).  
**Delivers:** Document describing framework vs Hub; SDK as documented framework surface; Hub audited to use only core/facade; optional boundary check in CI.  
**Addresses:** Clear “what devs depend on,” Hub as one app, documented boundary (FEATURES.md).  
**Avoids:** Pitfall 5 (framework separation breaking Hub); Pitfall 9 (re-export bloat — document stable facade).

### Phase 2: Thorough Testing (coverage, flakiness, integration/E2E)

**Rationale:** Improves confidence before adding native BrainFlow (which can introduce hardware/CI variability); clear ownership from Phase 1 for “where tests live.”  
**Delivers:** cargo-nextest and optional nextest.toml; more integration tests (neurohid-core, optionally neurohid-hub); Python coverage and flakiness fixes (pytest-rerunfailures where needed); E2E only where valuable; test tiers and isolation policy documented.  
**Uses:** cargo-nextest, cargo-llvm-cov (existing), pytest-rerunfailures (optional).  
**Avoids:** Pitfalls 1, 2, 6 (Rust/Python flakiness, E2E assumptions); Pitfall 8 (coverage gates hiding flakiness — fix or quarantine, don’t retry by default).

### Phase 3: Native BrainFlow — First-Class (docs, examples, Hub UX)

**Rationale:** Docs and Hub UX first give users a path and don’t block on SDK build details.  
**Delivers:** BrainFlow docs (NeuroHID + BrainFlow, build order, simulation vs native); runnable examples (synthetic board); Hub discovery/connection UX for BrainFlow (discover, connect, disconnect in Devices screen) as peer to LSL/Mock.  
**Implements:** Docs and examples; Hub devices screen changes; BrainFlowConfig and discovery in core unchanged.  
**Avoids:** Pitfall 3 (simulation vs native confusion); Pitfall 10 (Hub UX parity for BrainFlow).

### Phase 4: Native BrainFlow — Deeper (real SDK, board config, streaming)

**Rationale:** After first-class UX and tests are in place; real SDK behind feature flag with simulation retained.  
**Delivers:** Real BrainFlow SDK in neurohid-device under `brainflow-native` (git dep, build BrainFlow core first); Device/DeviceProvider implementation with BoardShim; optional board-specific config in Hub; streaming path BrainFlow → pipeline; synthetic board in CI.  
**Uses:** BrainFlow git dep, build-from-source docs, optional CI job for native build.  
**Avoids:** Pitfall 4 (API/build mismatches — pin version, document build, map rows, synthetic board in CI).

### Phase Ordering Rationale

- Separation is independent and low-cost; it clarifies ownership for tests and keeps BrainFlow clearly in the device layer.
- Testing before deeper BrainFlow reduces regressions when adding native SDK and avoids flakiness masking.
- BrainFlow in two waves (first-class then deeper) avoids blocking on SDK/build while delivering user value (docs, Hub UX) early.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 4 (Deeper BrainFlow):** BrainFlow Rust binding version/dep alignment (ndarray etc.); CI job that builds BrainFlow then neurohid-device; row layout and board metadata mapping — consider `/gsd:research-phase` if build or compat issues arise.

Phases with standard patterns (skip research-phase):
- **Phase 1 (Framework–Hub):** In-repo boundary and docs; crate-boundaries and codebase already describe it.
- **Phase 2 (Testing):** nextest, pytest-rerunfailures, and flakiness patterns are well documented.
- **Phase 3 (BrainFlow first-class):** Docs and Hub UX follow existing device-backend patterns; BrainFlow official docs sufficient.

## Confidence Assessment

| Area       | Confidence | Notes |
|-----------|------------|--------|
| Stack     | HIGH (testing, framework); MEDIUM (BrainFlow Rust) | nextest, llvm-cov, pytest-rerunfailures and framework-as-docs are well sourced; BrainFlow Rust path is official but version/dep alignment needs care. |
| Features  | MEDIUM     | Ecosystem and BrainFlow docs; testing/framework patterns from multiple sources; prioritization from PROJECT.md and feature research. |
| Architecture | HIGH   | Codebase and planning docs; no new external ecosystem; integration points and build order are clear. |
| Pitfalls  | MEDIUM    | Codebase + BrainFlow docs + Rust/Python testing literature; some integration pitfalls inferred from patterns. |

**Overall confidence:** HIGH for phase order and boundary/testing strategy; MEDIUM for BrainFlow native implementation details (build, deps, row mapping).

### Gaps to Address

- **BrainFlow Rust deps:** In-tree brainflow crate uses older ndarray etc.; wrap BoardShim in neurohid-device types and keep brainflow-native feature from leaking into main API surface; validate during Phase 4.
- **Validation harness in CI:** PROJECT.md defers RUNT-04/RUNT-05; align with “thorough testing” in a follow-up or later phase; no block for v1.1 roadmap.
- **IPC protocol impact:** Any change touching IPC (BrainFlow events, framework re-exports of IPC types) must trigger protocol verification and dual-side update; keep in impact checklist during planning.

## Sources

### Primary (HIGH confidence)

- STACK.md — cargo-nextest, cargo-llvm-cov, pytest-rerunfailures, BrainFlow build path and rust_package, framework boundary (docs/crate-boundaries.md).
- ARCHITECTURE.md — Integration points, build order, crate ownership (codebase and planning docs).
- PITFALLS.md — Flakiness causes, BrainFlow lifecycle, framework boundary enforcement (codebase + BrainFlow docs).

### Secondary (MEDIUM confidence)

- FEATURES.md — Table stakes, differentiators, anti-features (BrainFlow ecosystem, testing/framework patterns).
- Rust flaky tests study (async ~34%, concurrency ~25%); Python flakiness and determinism guidance.
- BrainFlow: Supported Boards, BuildBrainFlow, Data Format, adding new boards.

### Tertiary (validation during implementation)

- BrainFlow Rust crate ndarray/serde alignment with workspace; optional neurohid-framework facade crate if single dependency name needed later.

---
*Research completed: 2026-02-21*  
*Ready for roadmap: yes*
