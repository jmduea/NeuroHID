# Phase 9: BrainFlow First-Class — Research

**Researched:** 2026-02-21  
**Domain:** BrainFlow integration (simulation/synthetic board), device abstraction, Hub UX, docs and examples  
**Confidence:** HIGH (codebase + PITFALLS.md + official BrainFlow synthetic board docs)

## Summary

Phase 9 makes BrainFlow a first-class path in NeuroHID: first-class documentation, at least one runnable example using BrainFlow’s synthetic board end-to-end, Hub Devices screen parity with LSL (discover, connect, disconnect), preservation of the existing DeviceProvider/Device abstraction, and **replacing the in-repo mock device** with the BrainFlow synthetic board for tests, examples, and CI.

The codebase already has a BrainFlow **simulation** adapter in `neurohid-device` (no real SDK): `BrainFlowProvider` / `BrainFlowDevice` wrap the existing mock generator with BrainFlow-style board metadata (board_id 0 = Synthetic, 1 = Cyton, 2 = Ganglion). Discovery, `create_provider`, Hub backend selector, and Settings (board id, serial port) are already wired; the main gaps are: (1) **brainflow feature** not enabled by default for the Hub binary so selecting BrainFlow can fail at runtime; (2) **first-class docs** (setup, config, synthetic vs native, build order) missing; (3) **no runnable example** using BrainFlow synthetic; (4) **Devices screen** copy and flow are LSL-first (“Serial/BrainFlow parity is planned”); (5) **Mock** is still the default non-hardware path in tests, examples, and Auto fallback—synthetic board must replace it as the single non-hardware path.

**Primary recommendation:** Enable BrainFlow (synthetic) by default in the build path used by Hub and examples; add a dedicated BrainFlow doc and one runnable example; update Devices screen copy and ensure BrainFlow backend gets the same discover→connect→disconnect flow as LSL; then migrate all mock-only usage (tests, examples, CI, Auto fallback) to BrainFlow synthetic so the in-repo mock backend is no longer the canonical non-hardware path.

---

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| BRAIN-01 | User or developer can read first-class documentation for using BrainFlow with NeuroHID (setup, config, synthetic vs native hardware, build order) | Standard Stack: existing `BrainFlowConfig` (board_id, serial_port); docs placement in `docs/` (e.g. `docs/brainflow.md` or under user-guide); PITFALLS: label “simulation” vs “native”; build order only needed for Phase 10 native SDK. |
| BRAIN-02 | User can run at least one runnable example using BrainFlow’s synthetic board that demonstrates BrainFlow with NeuroHID end-to-end | Codebase: `BrainFlowProvider` with `board_id: 0` = Synthetic; discovery returns one `DeviceInfo`; connect → start_streaming works. Example: SDK or neurohid-core example that sets `DeviceBackend::BrainFlow`, `brainflow.board_id = 0`, runs discover→connect→stream. |
| BRAIN-03 | User can discover and connect BrainFlow devices from the Hub Devices screen with UX parity to LSL and other backends (discover, connect, disconnect) | Discovery task uses `create_provider(config)` → single provider; when backend is BrainFlow, `scan(provider)` fills `discovered_streams`; Rescan/Connect/Disconnect already use same commands. Gaps: default build must include `brainflow` feature; Devices screen copy and “LSL-first” label should include BrainFlow; Settings already have BrainFlow + board id/serial. |
| BRAIN-04 | BrainFlow remains one backend behind the existing DeviceProvider/Device abstraction; device-agnostic API is preserved | Already satisfied: `BrainFlowProvider`/`BrainFlowDevice` implement traits in `neurohid-device`; `create_provider` returns `Box<dyn DeviceProvider>`; no API changes required. |
| BRAIN-05 | BrainFlow’s synthetic board fully replaces the in-repo mock device: tests, examples, and CI use the synthetic board as the single non-hardware device path (no separate mock backend) | Replace: `DeviceBackend::Mock` and `MockProvider`/`MockDevice` usage in tests (neurohid-core runtime tests, neurohid-service tests, neurohid-sdk tests, neurohid-device mock tests), SDK example `embedded_runtime`, and `Auto` fallback (LSL → BrainFlow synthetic instead of LSL → Mock). Keep `mock.rs` only for internal use by BrainFlow adapter or deprecate/remove after BrainFlow device implements its own generator. |

</phase_requirements>

---

## Standard Stack

### Core (unchanged)

| Component | Purpose | Why Standard |
|-----------|---------|--------------|
| `neurohid-device` (BrainFlow adapter) | `BrainFlowProvider` / `BrainFlowDevice` behind `DeviceProvider`/`Device` | Already implemented; wraps mock with BrainFlow board metadata; no real SDK in Phase 9. |
| `neurohid-types::config::BrainFlowConfig` | `board_id` (default 0 = synthetic), `serial_port` | Single config type for device backend. |
| `neurohid-types::config::DeviceBackend::BrainFlow` | Backend selector in config and Hub | Already in `DeviceBackend::ALL` and Settings dropdown. |

### Build / feature

| Item | Version / value | Purpose |
|------|-----------------|---------|
| `neurohid-device` feature `brainflow` | Optional, currently off by default | Enables `brainflow` module and `BrainFlowProvider`/`BrainFlowDevice`. |
| `neurohid-core` feature `brainflow` | `neurohid-device/brainflow` | Required so `create_brainflow_provider` is compiled; **Phase 9:** include in default or in the feature set used by `neurohid` binary so Hub can use BrainFlow. |

**Recommendation:** Add `brainflow` to `neurohid-core` default features (e.g. `default = ["device-lsl", "brainflow"]`) so `cargo run -p neurohid` and Hub can discover/connect BrainFlow without a custom build. No new libraries; Phase 9 uses only the existing simulation adapter.

### Alternatives considered

| Instead of | Could use | Tradeoff |
|------------|-----------|----------|
| BrainFlow simulation (current adapter) | Real BrainFlow SDK (Phase 10) | Phase 9 scope is first-class docs/examples/Hub/CI on synthetic only; native SDK is BRAIN-06–08. |
| Replacing mock everywhere with BrainFlow synthetic | Keeping Mock and adding BrainFlow as extra | REQUIREMENTS: “synthetic board fully replaces the in-repo mock device”; single non-hardware path simplifies docs and CI. |

---

## Architecture Patterns

### Current device flow (preserve)

- **Config:** `DeviceConfig.backend` + `DeviceConfig.brainflow` (optional) → `create_provider()` in `neurohid-core/src/tasks/device/discovery.rs`.
- **Interactive (Hub):** DeviceTask holds one `Box<dyn DeviceProvider>`, runs `scan(provider, …)` on Rescan or periodically when no streams connected; `discovered_streams` in `ServiceState`; Connect/Disconnect by stream id.
- **BrainFlow:** `BrainFlowProvider::discover()` returns one `DeviceInfo` (from normalized metadata for `config.board_id`); `connect(device_id)` returns `BrainFlowDevice` (inner `MockDevice`). Same `DeviceId`/`source_id` shape as other backends.

### Recommended doc layout

- **First-class BrainFlow doc:** One canonical doc (e.g. `docs/brainflow.md` or section in `docs/user-guide.md`) covering: setup (enable BrainFlow in build if ever not default), config (backend = BrainFlow, board_id = 0 for synthetic, optional serial_port), synthetic vs native (current = simulation only; native = Phase 10), build order (only for Phase 10; Phase 9 no C++ build). Link from `docs/index.md`.
- **Stream semantics:** `docs/formats/stream-semantics.md` already has a BrainFlow row; clarify that “current implementation” is simulation adapter, timestamps/semantics align with mock.

### Hub Devices screen parity

- **Current:** Shows `discovered_streams`; Rescan, Connect, Disconnect by stream id; backend chosen in Settings. When backend is BrainFlow and feature is enabled, scan returns BrainFlow device(s); connect/disconnect use same commands.
- **Changes:** (1) Ensure binary is built with `brainflow` so selecting BrainFlow doesn’t hit “BrainFlow backend requires the `brainflow` feature”. (2) Update copy: e.g. “Available Streams (LSL / BrainFlow / …)” or “Streams” without “LSL-first” only; remove “Serial/BrainFlow parity is planned”. (3) Optional: backend indicator per stream so user sees “BrainFlow” vs “LSL” when multiple backends are supported later (Phase 9 can keep single-provider model).

### Synthetic board as sole non-hardware path (BRAIN-05)

- **Tests:** Replace `DeviceBackend::Mock` and `MockProvider`/`MockDeviceConfig` with `DeviceBackend::BrainFlow` and `BrainFlowConfig { board_id: 0, serial_port: None }` in neurohid-core runtime tests, neurohid-service tests, neurohid-sdk tests. neurohid-device’s own `mock.rs` tests can remain for the mock type in isolation, or be converted to BrainFlow adapter tests (board_id 0).
- **Examples:** e.g. `embedded_runtime`: set `config.device.backend = DeviceBackend::BrainFlow`, `config.device.brainflow = Some(BrainFlowConfig { board_id: 0, .. })`, and build with brainflow feature (or rely on default).
- **Auto backend:** Today `Auto` = LSL then Mock. Change to LSL then BrainFlow synthetic: e.g. `AutoProvider` delegates to LSL first, then to `BrainFlowProvider::new(BrainFlowConfig { board_id: 0, serial_port: None })` instead of Mock. That makes synthetic board the single fallback when no LSL streams.
- **CI:** Any job that currently uses Mock (e.g. extension E2E, service tests) should use BrainFlow synthetic instead; no separate mock backend in CI.

---

## Don't Hand-Roll

| Problem | Don't build | Use instead | Why |
|--------|-------------|-------------|-----|
| Synthetic data for tests/examples | New mock backend or ad-hoc generator | BrainFlow adapter with `board_id: 0` | Single non-hardware path; same Device/Sample pipeline as other backends. |
| BrainFlow board metadata (channels, rate) | Hardcoded per board in Hub | Existing `normalize_metadata()` in `brainflow.rs` (board_id → channels, sampling_rate_hz, names) | Already matches Cyton/Ganglion/Synthetic; extend only if new boards added. |
| Docs for “how to use BrainFlow” | Scattered snippets | One doc (e.g. `docs/brainflow.md`) linked from index | Single place for setup, config, synthetic vs native, build order (Phase 10). |

---

## Common Pitfalls

### Pitfall 1: BrainFlow “simulation” vs “native” confusion (PITFALLS.md §3)

- **What goes wrong:** Docs or UX imply “BrainFlow” = real SDK/hardware; later Phase 10 native integration breaks expectations or vice versa.
- **Avoid:** In Phase 9 docs and UI, label clearly: “BrainFlow (synthetic)” or “BrainFlow simulation” for current adapter; “BrainFlow native” reserved for Phase 10. First-class doc should have a “Synthetic vs native” subsection.

### Pitfall 2: Hub selects BrainFlow but binary built without `brainflow` feature

- **What goes wrong:** User selects BrainFlow in Settings; on Rescan or Connect, runtime returns “BrainFlow backend requires the `brainflow` feature”.
- **Avoid:** Include `brainflow` in the default feature set of the crate used by the Hub binary (e.g. neurohid-core default), or document and document a single recommended build command that enables `brainflow` for Hub.

### Pitfall 3: Auto fallback still using Mock after BRAIN-05

- **What goes wrong:** Requirements say synthetic board replaces mock, but `Auto` still falls back to `MockProvider`, so mock remains the main non-hardware path.
- **Avoid:** When implementing BRAIN-05, change `AutoProvider` to fall back to BrainFlow synthetic (BrainFlowProvider with board_id 0), not Mock. Remove or restrict Mock from “default” user-facing paths.

### Pitfall 4: Examples/tests still using Mock after BRAIN-05

- **What goes wrong:** Tests and examples keep using `DeviceBackend::Mock` or `MockProvider`, so CI and runnable examples don’t use the single non-hardware path.
- **Avoid:** Plan a full sweep: all runtime/service/SDK tests and at least one runnable example switch to `DeviceBackend::BrainFlow` + `board_id: 0`; CI uses same.

---

## Code Examples

### Creating BrainFlow provider (existing pattern)

```rust
// neurohid-core/src/tasks/device/discovery.rs (existing)
#[cfg(feature = "brainflow")]
fn create_brainflow_provider(config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    let brainflow_config = config.brainflow.clone().unwrap_or_default();
    Ok(Box::new(BrainFlowProvider::new(brainflow_config)))
}
```

### Default BrainFlowConfig for synthetic (existing)

```rust
// neurohid-types/src/config.rs
fn default_brainflow_board_id() -> i32 {
    0  // Synthetic board
}
impl Default for BrainFlowConfig {
    fn default() -> Self {
        Self {
            board_id: default_brainflow_board_id(),
            serial_port: None,
        }
    }
}
```

### Example: use BrainFlow synthetic in an example (target pattern)

```rust
// e.g. embedded_runtime or new brainflow_example
config.device.backend = DeviceBackend::BrainFlow;
config.device.brainflow = Some(BrainFlowConfig {
    board_id: 0,  // Synthetic
    serial_port: None,
});
// Then RuntimeBuilder::new(config).start() → RescanStreams → discover shows one stream; Connect → stream.
```

---

## State of the Art

| Old / current | Target (Phase 9) | Impact |
|---------------|------------------|--------|
| Mock as default non-hardware path in tests/examples/Auto | BrainFlow synthetic (board_id 0) as single non-hardware path | One path to document and maintain; no “mock vs BrainFlow” split. |
| No first-class BrainFlow doc | One doc: setup, config, synthetic vs native, build order (for Phase 10) | Users and contributors have a single reference. |
| Devices screen “LSL-first”, “Serial/BrainFlow parity planned” | BrainFlow as peer: same discover/connect/disconnect, copy updated | BRAIN-03 UX parity. |
| brainflow feature optional (Hub may lack it) | brainflow in default build for Hub | Selecting BrainFlow in Settings works out of the box. |

**Deprecated / to avoid:** Documenting “BrainFlow” without distinguishing simulation (Phase 9) vs native (Phase 10). Using Mock in new tests or examples once BRAIN-05 is done.

---

## Open Questions

1. **Auto fallback order (LSL → BrainFlow vs LSL → Mock)**  
   - Requirement: synthetic replaces mock. So Auto should become LSL then BrainFlow synthetic. Confirm no product need to keep Mock in Auto for Phase 9.

2. **Retention of `mock.rs` and `MockProvider`**  
   - BrainFlow adapter currently *uses* `MockDevice` internally. Options: (a) keep `mock.rs` as internal implementation detail of BrainFlow adapter only, no longer exposed as a user/backend choice; (b) move synthetic generation into `brainflow.rs` and deprecate MockProvider. Research recommends (a) for Phase 9 to minimize churn; (b) can be Phase 10 or later.

3. **Board selector in Hub for BrainFlow**  
   - Settings already have board_id (drag value) and serial port. For Phase 9, “at least one runnable example” and “discover and connect” can rely on default board_id 0. A nicer “Synthetic / Cyton / Ganglion” dropdown can be a follow-up if needed.

---

## Sources

### Primary (HIGH confidence)

- Codebase: `crates/neurohid-device/src/brainflow.rs`, `mock.rs`, `traits.rs`; `crates/neurohid-core/src/tasks/device/discovery.rs`, `mod.rs`; `crates/neurohid-hub/src/screens/devices.rs`, `settings/device.rs`; `crates/neurohid-types/src/config.rs` (BrainFlowConfig, DeviceBackend).
- `.planning/research/PITFALLS.md` — Pitfalls 3, 4, 10; simulation vs native; Hub UX parity; synthetic board in CI.
- `.planning/REQUIREMENTS.md` — BRAIN-01–05.
- BrainFlow Supported Boards / synthetic board: board_id 0 = SYNTHETIC_BOARD; prepare_session → start_stream → get_board_data → stop_stream (verified via web search 2026).

### Secondary (MEDIUM confidence)

- `docs/formats/stream-semantics.md` — BrainFlow row; current implementation is simulation.
- `docs/architecture-rust-core.md`, `docs/extension-contracts.md` — Device backends and BrainFlow listed.

### Tertiary (LOW confidence)

- BrainFlow Rust bindings / C++ build order: relevant for Phase 10 (BRAIN-06–08), not Phase 9.

---

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH — existing code and types; only feature-default and doc/example/UX changes.
- Architecture: HIGH — same DeviceProvider/Device flow; BrainFlow already integrated; replacement of Mock by synthetic is a clear migration.
- Pitfalls: HIGH — PITFALLS.md and codebase inspection; Hub feature default and simulation vs native labeling are the main risks.

**Research date:** 2026-02-21  
**Valid until:** ~30 days (stable scope; Phase 10 may add native SDK details).

---

## RESEARCH COMPLETE

**Phase:** 9 — BrainFlow First-Class  
**Confidence:** HIGH

### Key findings

- BrainFlow simulation adapter and config already exist; Hub Settings and discovery/connect flow support BrainFlow; gap is default build (brainflow feature), first-class doc, one runnable example, Devices copy, and replacing Mock with synthetic everywhere.
- Synthetic board (board_id 0) is the standard no-hardware path; use it for tests, examples, CI, and Auto fallback so the in-repo mock is no longer the canonical non-hardware backend.
- Keep DeviceProvider/Device as the only device API; no new backends or hand-rolled mocks.

### File created

`c:\dev\neurohid\.planning\phases\09-brainflow-first-class\09-RESEARCH.md`

### Ready for planning

Planner can create PLAN.md files from this research.
