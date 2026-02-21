# Feature Research: v1.1 Testing, BrainFlow & Framework Separation

**Domain:** NeuroHID v1.1 — thorough testing, native BrainFlow integration, framework-vs-Hub separation  
**Researched:** 2026-02-21  
**Confidence:** MEDIUM (ecosystem + official BrainFlow docs; testing/framework patterns from multiple sources)

## Scope

This document covers **only** the three new feature areas for the v1.1 milestone. Existing v1.0 capabilities (device discovery/connection, signal pipeline, decoder inference, HID output, IPC, Hub GUI, headless runtime, config/profile storage, calibration, SDK facade, validation harness, extension contracts) are treated as dependencies where relevant.

---

## 1. Thorough Testing

### Table Stakes (Users Expect These)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Unit tests for new/changed code** | CI and PRs assume tests exist; missing tests block confidence. | LOW | Rust: `cargo test --workspace`; Python: `uv run --project python pytest python/tests`. Already in CI. |
| **Deterministic tests (no flakiness)** | Flaky tests erode trust and waste CI; developers expect green-or-red. | MEDIUM | Main causes: async/waits, concurrency, network, shared state. Prevention: avoid hard sleeps; use condition-based waits; isolate state. |
| **Integration tests at boundaries** | Catches serialization, IPC, and interface mismatches that unit tests miss. | MEDIUM | Valuable at: Rust↔Python IPC, device→signal→decoder→action pipeline, config load/save. |
| **CI gates that reflect reality** | Passing CI should mean “safe to merge” for the scope exercised. | LOW–MEDIUM | Today: `cargo test`, pytest, validation harness invocable. Broader/deeper coverage and flakiness reduction make the gate meaningful. |

### Differentiators (Competitive Advantage)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **E2E tests where they add value** | Validates full user flow (e.g. Hub discover→connect→stream or runtime profile→decoder→action) without over-investing. | HIGH | Keep E2E minimal; use for critical paths only. Validation harness (Soak, LatencyMatrix, BootMatrix) is existing E2E-style asset. |
| **Stable validation harness in CI** | Soak/LatencyMatrix/BootMatrix run in CI so regressions in runtime behavior are caught. | MEDIUM | Depends on: neurohid-service binary, control port, optional Python bridge. Deferred per PROJECT.md but aligns with “thorough testing.” |
| **Rust + Python coverage story** | Single story for “what we test” across the monorepo. | MEDIUM | Document what is unit vs integration vs E2E; which tests run per commit vs nightly. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|--------------|
| **E2E for every feature** | “Test like a user.” | Slow, flaky, high maintenance; blocks quick feedback. | Test pyramid: many unit, some integration, few E2E for critical paths only. |
| **Hard-coded sleeps in tests** | Simple way to “wait for async.” | Primary cause of flakiness; fails under load or slow CI. | Condition-based waits, test doubles, or bounded retries with clear success criteria. |
| **Testing mock behavior** | Easy to make green. | Doesn’t validate real behavior; false confidence. | Test real behavior; use mocks only to isolate boundaries (and verify real contracts at integration). |
| **100% coverage target** | “Fully tested.” | Diminishing returns; can push logic into tests or discourage refactors. | Target critical paths and boundaries; measure coverage for visibility, not as a single goal. |

### Dependencies on Existing NeuroHID Capabilities

- **Device/signal/decoder/action pipeline** — Integration tests need a runnable pipeline (mock device or LSL/BrainFlow simulation).
- **Rust↔Python IPC** — Integration tests for bridge/control depend on `neurohid-ipc` and Python `neurohid_ml` client.
- **Validation harness** — Soak/LatencyMatrix/BootMatrix depend on `neurohid-service` binary and control protocol; extending or running them in CI builds on existing harness.
- **Config/profile storage** — Integration tests that load profiles depend on `neurohid-storage` and documented config format.

---

## 2. Native BrainFlow Integration

### Table Stakes (Users Expect These)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **First-class documentation** | BCI/EEG tooling is complex; BrainFlow users expect docs on how NeuroHID uses BrainFlow (board IDs, params, session lifecycle). | LOW–MEDIUM | Point to BrainFlow official docs; add NeuroHID-specific: “using BrainFlow with NeuroHID,” config fields, troubleshooting. |
| **Runnable examples** | BrainFlow ecosystem emphasizes examples (e.g. BoardShim + params → prepare_session → start_stream → get_board_data). | LOW–MEDIUM | At least one example: discover/connect/stream with a real or synthetic BrainFlow board, matching BrainFlow’s own pattern. |
| **Hub discovery/connection UX for BrainFlow** | Users expect to discover and connect BrainFlow devices from the Hub like LSL streams. | MEDIUM | Depends on: Hub devices screen (today LSL-oriented), ServiceManager, control protocol. BrainFlow discovery uses board_id + params (serial_port, ip_address, timeout, etc.). |
| **Device-agnostic API preserved** | Existing DeviceProvider/Device abstraction must still hold; BrainFlow is one backend. | LOW | Already the case: `neurohid-device` has BrainFlow adapter (currently simulation); real SDK behind `brainflow` feature. |

### Differentiators (Competitive Advantage)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Board-specific config in Hub** | Per-board params (serial port, IP, timeout) and presets in UI instead of raw config edits. | MEDIUM–HIGH | Depends on BrainFlow board catalogue (e.g. Supported Boards), `BrainFlowConfig` in neurohid-types, and Hub settings/device UX. |
| **Unified streaming path (BrainFlow → pipeline)** | BrainFlow streams feed the same signal pipeline as LSL/mock; one path for decoding and actions. | MEDIUM | Real BrainFlow SDK integration: session lifecycle (prepare_session/start_stream/stop_stream/release_session) mapped to Device/SampleStream; channel config from board metadata. |
| **Synthetic/dummy board in CI** | BrainFlow provides synthetic/playback boards; using them in CI avoids hardware. | LOW–MEDIUM | Enables integration tests and Hub E2E without physical devices. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|--------------|
| **BrainFlow-only Hub** | “Simplify for one backend.” | Breaks multi-backend (LSL, Mock, Serial) and device-agnostic value. | Keep LSL + BrainFlow + Mock in one devices UX; BrainFlow as first-class, not only. |
| **Copy-paste of BrainFlow examples without abstraction** | Quick to wire. | Bypasses DeviceProvider/Device; duplicates lifecycle and config. | Wrap BrainFlow in existing traits; document how BoardShim/params map to NeuroHID config and discovery. |
| **Real hardware required for docs/examples** | “Test on real device.” | Blocks contributors and CI. | Prefer synthetic/playback board in examples and CI; document real hardware as optional. |

### Dependencies on Existing NeuroHID Capabilities

- **DeviceProvider/Device traits** — BrainFlow backend implements same interface as LSL/Mock/Serial; current simulation adapter in `neurohid-device/src/brainflow.rs` is the pattern; real SDK behind `brainflow` feature.
- **Discovery and core** — `neurohid-core` discovery (e.g. `tasks/device/discovery.rs`) and runtime must treat BrainFlow as a discoverable backend with config (e.g. `BrainFlowConfig`, board_id, params).
- **Hub devices screen** — Today shows `ControlSnapshot.discovered_streams` (LSL-oriented). BrainFlow discovery/connection must appear there (rescan, connect, disconnect) via same control protocol.
- **Config and types** — `neurohid-types` already has `BrainFlowConfig`; storage and profile must persist BrainFlow board_id and connection params.
- **SDK** — `neurohid-sdk` already has `device-brainflow` feature; first-class BrainFlow means documented, stable, and usable from SDK + examples.

---

## 3. Framework-vs-Hub Separation

### Table Stakes (Users Expect These)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Clear “what devs depend on”** | Embedders and SDK users need a single, stable surface (crates, features, docs) so they don’t depend on Hub internals. | MEDIUM | Today: `neurohid-sdk` with feature-gated re-exports. Refine so “framework” = types + chosen components + core facade; document it explicitly. |
| **Hub as one application** | Hub is “our app” on top of the framework; its code and deps are distinct from the framework surface. | MEDIUM | Structural boundary in-repo: Hub depends on framework (core + facade), not on every component crate directly where avoidable. Crate-boundaries already say: core → (hub \| sdk \| binary). |
| **Documented boundary** | Contributors and users need to know what is framework vs Hub. | LOW | Docs: “Framework surface” (SDK + core facade + listed crates) vs “Hub application” (neurohid-hub, binaries that launch Hub/service/validate). |
| **No reverse coupling** | Component crates must not depend on Hub or GUI. | LOW | Already in `docs/crate-boundaries.md`: types → components → core → (hub \| sdk \| binary). |

### Differentiators (Competitive Advantage)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Framework as distinct crate set or layout** | Enables future split (e.g. framework in own repo) and clearer versioning. | MEDIUM | In-repo: framework = e.g. neurohid-types + neurohid-device/signal/platform/ipc/storage/calibration + neurohid-core, with a single “framework” or SDK entrypoint; Hub and neurohid binary depend only on that + neurohid-hub. |
| **Stable SDK feature matrix** | Embedders choose only what they need (device, signal, runtime, no Hub). | LOW | Already present (`neurohid-sdk` features); document and stabilize so “framework” = SDK default/recommended feature set without Hub. |
| **Single binary still works** | neurohid (Hub), neurohid-service, neurohid-validate remain the shipped products. | LOW | Separation is structural and dependency-direction, not necessarily more binaries. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|--------------|
| **Hub depending on every component directly** | “Hub needs device/signal/ipc.” | Blurs framework boundary; embedders can’t depend on “just runtime” without pulling Hub. | Hub uses core facade (and core pulls components); SDK exposes same facade with feature flags. |
| **Two separate repos in v1.1** | “Clean split.” | PROJECT.md says full repo split is planned for later milestone. | In-repo structural boundary first; document path to future repo split. |
| **“Framework” as a single crate that re-exports everything** | Simpler dependency. | Can create a god-crate and hide component boundaries. | Prefer: clear set of crates that form the framework + one SDK (or facade) crate that re-exports; Hub depends on that set. |
| **Breaking existing binaries** | “Pure framework.” | Compatibility constraint: keep neurohid, neurohid-service, neurohid-validate. | Same binaries; dependency graph and docs change, not entry points. |

### Dependencies on Existing NeuroHID Capabilities

- **neurohid-sdk** — Already the public facade; framework separation formalizes “framework = what SDK exposes (optionally without hub)” and possibly a documented “framework” subset of workspace.
- **neurohid-core** — Orchestration and facade; Hub and runtime binaries already depend on core. Core must remain the composition layer that pulls in components.
- **docs/crate-boundaries.md** — Already defines layers (types → components → core → hub \| sdk \| binary); update to name “framework” and “Hub application” explicitly.
- **Cargo workspace** — No need to move crates physically yet; dependency direction and documentation define the boundary.

---

## Feature Dependencies (v1.1)

```
Thorough testing
    ├── depends on: Device/signal/decoder pipeline, IPC, config/storage, validation harness
    └── enables: Confidence for BrainFlow and framework changes

Native BrainFlow (docs/examples + Hub UX)
    ├── depends on: DeviceProvider/Device, BrainFlowConfig, Hub devices screen, ServiceManager, control protocol
    └── enables: Deeper integration (board config, real streaming) in same or later milestone

Native BrainFlow (deeper: board config, streaming)
    └── depends on: First-class docs + examples + Hub discovery/connection UX, real BrainFlow SDK behind feature

Framework-vs-Hub separation
    ├── depends on: neurohid-sdk, neurohid-core, crate-boundaries
    └── enables: Clear embedder story; future repo split
```

### Cross-Cutting

- **Testing** does not block BrainFlow or framework separation, but broader tests and flakiness reduction reduce risk when adding BrainFlow and refactoring boundaries.
- **BrainFlow** and **framework separation** are independent: either can proceed in parallel; Hub UX for BrainFlow should use the same “framework” surface (core/discovery) that separation clarifies.

---

## MVP Definition (v1.1)

### Launch With (v1.1)

- [ ] **Thorough testing** — Broader and deeper coverage (Rust + Python); reduce flakiness (condition-based waits, isolation); add integration tests at IPC and pipeline boundaries; E2E only where valuable (e.g. one critical path).
- [ ] **Native BrainFlow (first wave)** — First-class docs (NeuroHID + BrainFlow); runnable examples (synthetic board); Hub discovery/connection UX for BrainFlow devices (discover, connect, disconnect in Devices screen).
- [ ] **Framework-vs-Hub separation** — Structural boundary in-repo: framework as distinct crate set or layout that Hub depends on; documented “framework surface” and “Hub as one app”; no new binaries; full repo split deferred.

### Add After v1.1 (as scope or follow-up)

- [ ] **BrainFlow deeper integration** — Board-specific config in Hub, real SDK behind `brainflow` feature, streaming path BrainFlow → pipeline; synthetic board in CI.
- [ ] **Validation harness in CI** — Soak/LatencyMatrix/BootMatrix in CI (PROJECT.md defers RUNT-04/RUNT-05 but aligns with thorough testing).

### Future Consideration

- [ ] **Framework in separate repo** — After in-repo boundary is stable and documented.

---

## Feature Prioritization Matrix (v1.1)

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Broader/deeper unit + integration tests | HIGH | MEDIUM | P1 |
| Flakiness reduction | HIGH | MEDIUM | P1 |
| E2E where valuable (minimal) | MEDIUM | MEDIUM–HIGH | P2 |
| BrainFlow docs + examples | HIGH | LOW–MEDIUM | P1 |
| Hub BrainFlow discovery/connection UX | HIGH | MEDIUM | P1 |
| BrainFlow board config + real streaming | MEDIUM | HIGH | P2 |
| Framework-vs-Hub structural boundary | HIGH | MEDIUM | P1 |
| Documented framework vs Hub | HIGH | LOW | P1 |

**Priority key:** P1 = must have for v1.1; P2 = should have within or right after v1.1.

---

## Sources

- BrainFlow: [Code Samples](https://brainflow.readthedocs.io/en/stable/Examples.html), [Supported Boards](https://brainflow.readthedocs.io/en/stable/SupportedBoards.html), [Installation](https://brainflow.readthedocs.io/en/stable/BuildBrainFlow.html) (MEDIUM confidence — official docs).
- Testing: Flakiness (async/waits, concurrency) — arXiv 2502.02760 (Rust flaky tests); E2E flakiness prevention (condition-based waits, isolation, avoid hard sleeps) — TestResult.co, LeadWithSkills, FullScale 2025 (MEDIUM confidence); test pyramid and when E2E adds value — CD Migration Guide, testing best practices (MEDIUM confidence).
- Framework vs app separation: Azure SDK repo structure, Fuchsia RFC-0241 (platform/external split), Kotlin Multiplatform module configuration (MEDIUM confidence — official or RFC).
- NeuroHID: PROJECT.md, crate-boundaries.md, STRUCTURE.md, neurohid-device brainflow.rs, neurohid-sdk Cargo.toml, neurohid-validate.rs, Hub devices.rs (HIGH confidence — repo).

---
*Feature research for: NeuroHID v1.1 (testing, BrainFlow, framework-vs-Hub)*  
*Researched: 2026-02-21*
