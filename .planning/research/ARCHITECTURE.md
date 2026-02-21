# Architecture Research: v1.1 Integration (Testing, BrainFlow, Framework–Hub Separation)

**Domain:** Integration of new milestone features into existing NeuroHID architecture  
**Researched:** 2026-02-21  
**Confidence:** HIGH (codebase and planning docs; no new external ecosystem)

## Purpose

This document answers how **(1) thorough testing**, **(2) native BrainFlow integration**, and **(3) framework–vs–Hub separation** integrate with the existing Rust/Python architecture. It identifies integration points, new vs modified components, data-flow impact, and a suggested build order for roadmap phases.

---

## Existing Architecture (Baseline)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  neurohid (bins)  neurohid-hub  neurohid-sdk                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│  neurohid-core (runtime, service, tasks, facade)                             │
├─────────────────────────────────────────────────────────────────────────────┤
│  neurohid-device  neurohid-signal  neurohid-platform  neurohid-ipc           │
│  neurohid-storage  neurohid-calibration                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│  neurohid-types                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Data flow (unchanged by v1.1 scope):**  
DeviceTask → SignalTask → DecoderTask → ActionTask → OutletTask; Hub/CLI use `RuntimeHandle` and `neurohid_core::facade` (IPC, storage). Python bridge over IPC for ML/ErrP/training.

---

## 1. Thorough Testing — Integration

### Integration Points

| Where | What | Notes |
|-------|------|--------|
| **Rust crates** | Co-located `#[cfg(test)] mod tests`; optional `tests/` for integration | Existing pattern; extend coverage and add integration tests where valuable |
| **neurohid-core** | `tests/` directory (e.g. `extension_outlet_e2e.rs`) | Already has one e2e-style integration test; add more IPC/service integration tests here |
| **neurohid-hub** | Unit tests in-screen or in app; optional integration tests | Real IPC connect/disconnect/reconnect tests referenced in CHANGELOG; reduce flakiness here |
| **Python** | `python/tests/` (pytest + unittest API); `python/pyproject.toml` (coverage) | Extend coverage, stabilize async/flaky tests |
| **CI** | `cargo test --workspace`; pytest with coverage; optional Rust coverage (e.g. tarpaulin) | Coverage reporting (Codecov) already in CHANGELOG; formalize gates and flakiness mitigation |

### New vs Modified Components

| Component | Status | Change |
|-----------|--------|--------|
| **Rust coverage tooling** | **New** (optional) | Add `cargo-tarpaulin` or similar to manifests/CI for coverage; no new crates |
| **Rust integration test layout** | **Modified** | More tests under `crates/neurohid-core/tests/` and possibly `crates/neurohid-hub/tests/`; no new crate |
| **Python tests** | **Modified** | Same layout (`python/tests/`); improve coverage, fixtures, and async stability |
| **CI workflow** | **Modified** | Stricter coverage gates, flakiness handling (retries, quarantine), optional Rust coverage upload |

### Data Flow Impact

**None.** Testing is cross-cutting: it exercises existing data flow and contracts. No change to DeviceTask → … → OutletTask or to IPC/bridge protocols. Integration tests may spawn real runtime or use in-process mocks (as `extension_outlet_e2e` does).

### Testing Layout and Ownership

- **Unit tests:** Remain co-located in each crate (`#[cfg(test)] mod tests`); ownership stays with the crate.
- **Integration tests:** Owned by the crate that composes the behavior (e.g. `neurohid-core` for pipeline/extension, `neurohid-core` or `neurohid-hub` for IPC). Prefer `neurohid-core/tests/` for runtime/service/IPC integration; `neurohid-hub/tests/` only if tests are Hub-specific (e.g. UI/control flow).
- **E2E:** Current pattern is in-process integration test (e.g. outlet e2e). Broader E2E (full pipeline, multiple processes) can live in a dedicated step in CI or a small `tests/e2e/`-style layout under repo root if needed later; not a new crate.
- **Python:** Keep all tests under `python/tests/`; ownership with Python package.

---

## 2. Native BrainFlow Integration — Integration

### Integration Points

| Where | What | Notes |
|-------|------|--------|
| **neurohid-types** | `BrainFlowConfig`, `DeviceBackend::BrainFlow` | **Existing.** Possibly extend with board-specific options (e.g. serial port, stream params) for deeper integration |
| **neurohid-device** | `brainflow.rs` (currently simulation adapter wrapping Mock) | **Modified.** Today: no real SDK; normalize board metadata only. Native: link BrainFlow SDK, implement `Device`/`DeviceProvider` with real `BoardShim` (prepare_session, start_stream, get_board_data) |
| **neurohid-core/tasks/device** | `create_brainflow_provider(config)` in `discovery.rs` | **Modified.** Already feature-gated; no signature change for “real” backend — same `Box<dyn DeviceProvider>` |
| **Hub (neurohid-hub)** | Devices screen: discovery, connection UX, backend dropdown | **Modified.** “Serial/BrainFlow parity” note; add BrainFlow discovery/connection UX and any board/config UI |
| **Docs** | Device docs, stream semantics, BrainFlow-specific how-to | **New** (docs only). Document BrainFlow as first-class backend; examples (config snippets, minimal connect flow) |
| **Python** | No direct BrainFlow in Python for v1.1 device path | Device path is Rust-only; Python bridge unchanged |

### New vs Modified Components

| Component | Status | Change |
|-----------|--------|--------|
| **neurohid-device** | **Modified** | `brainflow.rs`: add native SDK path behind same trait (feature “brainflow” can mean real SDK when enabled); keep simulation path for CI/docs where no hardware) |
| **neurohid-types** | **Modified** | Optional: extend `BrainFlowConfig` for board id, port, stream params if needed for native UX |
| **neurohid-core** | **Modified** | Discovery only if config shape changes; otherwise unchanged |
| **neurohid-hub** | **Modified** | Devices screen: BrainFlow discovery/connect UX, parity with LSL/Serial where applicable |
| **Docs + examples** | **New** | New or updated docs (BrainFlow setup, Hub UX); optional minimal example (e.g. config + connect) |

### Data Flow Impact

**No change to pipeline shape.** BrainFlow remains one more backend behind `DeviceProvider`/`Device`. Samples still flow: BrainFlowDevice → DeviceTask → SignalTask → … . Only the source of samples changes (real hardware vs current mock). Optional: if BrainFlow-specific stream params (e.g. chunk size) are exposed, they stay inside device/signal boundary.

### BrainFlow Placement

- **Ownership:** Device backend stays in **neurohid-device** (same as LSL, Serial, Mock). No new crate.
- **Feature gate:** Keep `brainflow` feature; when “native” is implemented, the same feature can enable real SDK (with optional “brainflow-mock” or env for CI without hardware).
- **Docs/UX first:** Implement documentation and Hub discovery/connection UX before or in parallel with deeper SDK wiring; avoids blocking on SDK details.

---

## 3. Framework–vs–Hub Separation — Integration

### Integration Points

| Where | What | Notes |
|-------|------|--------|
| **Repo layout** | Structural boundary: “framework” vs “Hub app” | Framework = types + component crates + core + SDK surface. Hub = neurohid-hub + neurohid binaries. Same repo; no split yet |
| **neurohid-sdk** | Public API for embedders | **Existing.** Becomes the documented “framework” surface; Hub must not rely on SDK-internal or Hub-only types from framework |
| **neurohid-core::facade** | Re-exports for IPC/storage so Hub doesn’t depend on neurohid-ipc/neurohid-storage directly | **Existing.** Enforces “Hub depends on core only” for runtime access |
| **neurohid-hub** | Depends only on neurohid-core (and transitively what core needs) | **Existing.** Already uses facade; no direct device/signal deps in production. Separation work is clarity and docs, not new deps |
| **Docs** | Document “framework” vs “Hub as one app” | **New.** Single doc or section: what is the framework (crates, facade, SDK), what is the Hub (app that uses framework) |

### New vs Modified Components

| Component | Status | Change |
|-----------|--------|--------|
| **Crate graph** | **Unchanged** | types → components → core → (hub | sdk | binary). No new crates; no dependency reversal |
| **neurohid-sdk** | **Modified (docs/features)** | Document as the framework entrypoint; ensure feature set (e.g. runtime, device, ipc) is the “official” embedder surface |
| **neurohid-hub** | **Modified (docs/clarity)** | No new deps; possibly audit that Hub does not reach into component crates except via core/facade |
| **Docs** | **New** | `docs/framework-and-hub.md` (or equivalent): framework boundary, crate map, “Hub is one app on top of framework” |

### Data Flow Impact

**None.** Separation is structural and documentary. Same data flow; same IPC and control; Hub still talks to runtime via RuntimeHandle and facade. Full framework split to another repo is out of scope for v1.1.

### Framework Boundary (In-Repo)

- **Framework:** neurohid-types, neurohid-device, neurohid-signal, neurohid-platform, neurohid-ipc, neurohid-storage, neurohid-calibration, neurohid-core, neurohid-sdk (and optionally neurohid-outlet-example as example consumer). “What devs build on.”
- **Hub:** neurohid-hub + neurohid binaries. “Our app” that uses the framework via neurohid-core and neurohid-core::facade.
- **Boundary rule:** Hub and other apps depend on the framework through neurohid-core (and neurohid-sdk for embedders). They do not depend on component crates directly for production code; facade and SDK re-exports are the boundary.

---

## Suggested Build Order for v1.1 Phases

Dependencies between the three features suggest this order:

1. **Framework–Hub separation (structural + docs)**  
   - **Rationale:** Defines the boundary that testing and BrainFlow will live within. No code flow change; documentation and optional dependency audit.  
   - **Deliverables:** Doc describing framework vs Hub; SDK as documented framework surface; Hub audited to use only core/facade.  
   - **Enables:** Clear ownership for “where do tests live” and “BrainFlow stays in device layer.”

2. **Thorough testing (coverage, flakiness, integration/E2E)**  
   - **Rationale:** Improves confidence before adding native BrainFlow (which can introduce hardware/CI variability).  
   - **Deliverables:** Rust coverage tooling and CI gates; more integration tests (e.g. IPC in neurohid-core/neurohid-hub); Python coverage and flakiness fixes; E2E only where valuable.  
   - **Enables:** Safe refactor of brainflow.rs (simulation vs native) with tests in place.

3. **Native BrainFlow (docs/UX then deeper)**  
   - **Rationale:** Docs and Hub UX first give users a path and don’t block on SDK details; deeper integration (real SDK, board config, streaming) follows.  
   - **Deliverables:** BrainFlow docs and examples; Hub discovery/connection UX for BrainFlow; then (as scope allows) real BrainFlow SDK in neurohid-device, with simulation path retained for CI.  
   - **Depends on:** Framework boundary (so BrainFlow stays clearly in device layer); testing (so new code is covered and CI stays stable).

**Phase ordering rationale:**  
- Separation is independent and cheap (docs + audit).  
- Testing benefits from a clear “framework” boundary for test ownership.  
- BrainFlow benefits from both: framework boundary keeps BrainFlow in one place; testing reduces regressions when adding native SDK.

---

## Integration Points Summary

### Internal Boundaries (v1.1-relevant)

| Boundary | Communication | v1.1 Note |
|----------|---------------|------------|
| Testing ↔ Crates | Tests live in crates or in crate `tests/` | No new cross-crate test crate; integration tests in neurohid-core (and optionally neurohid-hub) |
| BrainFlow ↔ Pipeline | DeviceProvider/Device → DeviceTask → SignalTask | Unchanged; BrainFlow is one more backend in neurohid-device |
| Framework ↔ Hub | neurohid-core (facade) + neurohid-sdk | Document and enforce; Hub uses only core/facade for runtime access |

### New vs Modified (Checklist)

| Area | New | Modified |
|------|-----|----------|
| Testing | Rust coverage in CI (optional); possibly e2e layout under repo root | neurohid-core/tests, neurohid-hub tests, Python tests, CI workflow |
| BrainFlow | Docs, examples | neurohid-device/brainflow.rs, neurohid-types (optional config), Hub devices screen |
| Framework–Hub | Doc (framework-and-hub) | neurohid-sdk docs, neurohid-hub audit, crate-boundaries.md |

---

## Anti-Patterns to Avoid

### Testing

- **Scattering integration tests across many crates:** Prefer a small number of “integration” owners (e.g. neurohid-core for runtime/IPC, neurohid-hub only if Hub-specific). Avoid a dedicated “integration test crate” that depends on everything.
- **E2E that require hardware or heavy processes in every CI run:** Prefer in-process integration tests (like extension_outlet_e2e); add full pipeline E2E only where value is clear and flakiness is controlled.

### BrainFlow

- **Putting BrainFlow logic in core or hub:** Keep all BrainFlow-specific code in neurohid-device behind the existing Device/DeviceProvider traits; core only calls `create_brainflow_provider(config)`.
- **Breaking the simulation path:** Keep the current mock-based BrainFlow adapter for CI and docs; native SDK can be a separate code path behind the same feature or a build-time choice.

### Framework–Hub

- **Hub depending on neurohid-device or neurohid-signal directly:** Hub should use neurohid-core (and facade) only for runtime/config; device/signal are framework internals.
- **New “framework” crate that just re-exports:** The framework is the existing set of crates (types → components → core) plus neurohid-sdk as the public surface; document that rather than adding a new meta-crate for v1.1.

---

## Sources

- `.planning/PROJECT.md` — v1.1 milestone and requirements
- `.planning/codebase/ARCHITECTURE.md` — Layers, tasks, data flow
- `.planning/codebase/TESTING.md` — Current test layout and patterns
- `docs/architecture-rust-core.md`, `docs/crate-boundaries.md` — Crate map and dependency rules
- `docs/integration-architecture.md` — Data flow and device backends
- `crates/neurohid-device/src/brainflow.rs` — Current BrainFlow (simulation) implementation
- `crates/neurohid-core/src/lib.rs` — Facade re-exports
- `.planning/codebase/CONCERNS.md` — E2E and coverage notes

---
*Architecture research for: v1.1 Testing, BrainFlow, Framework–Hub separation (integration only).*
