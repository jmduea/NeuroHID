# Phase 6: Composable and extensible - Research

**Researched:** 2026-02-21
**Domain:** Rust plugin/extension architecture, trait-based pipeline contracts, dynamic loading vs subprocess
**Confidence:** MEDIUM

## Summary

Phase 6 makes the four pipeline slots—acquisition, signal preprocessing, decoder, and output—swappable via published contracts, with config as source of truth and support for both in-process (Rust dylib) and subprocess (e.g. Python) implementations. The codebase already has a trait-based device layer (`DeviceProvider` in `neurohid-device`) and a single factory in `neurohid-core` that matches on `DeviceBackend` enum; outlet and decoder are currently concrete types with no published contract. Research supports: (1) defining contracts in `neurohid-types` (or published docs) for each slot; (2) extending config to select by name (string) for extensions while keeping enum for built-ins; (3) using `libloading` for in-process plugins and a line-based or JSON protocol over stdin/stdout for subprocess plugins; (4) discovery at startup plus explicit refresh, name-only ID, duplicate names = hard fail; (5) one example outlet plugin (minimal, e.g. log-only) tested in CI via build + e2e.

**Primary recommendation:** Publish one contract per slot in neurohid-types (or docs), add name-based selection and a discovery/registry layer in core; implement the outlet contract first and ship one example outlet plugin as a workspace member or dedicated directory, with CI building and running the runtime with that outlet.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **How "replace" works:** Config is source of truth. Hub shows discovered extensions when configuring device/outlet; CLI/headless stay config-only. Load/run: in-process (e.g. Rust/dylib) or subprocess (e.g. Python/external binary) allowed if implementation satisfies the same contract. Config when picking in Hub: always sync — choice in Hub updates config immediately. Custom component fails to load/run: fail clearly, no silent fallback; pipeline stopped or degraded with explicit reason. All four slots (acquisition, signal preprocessing, decoder, output) have a contract and selection in config/Hub. Config change while runtime running: apply on next start of that slot where design allows, without full restart. Where in Hub: both — Extensions screen to manage/install; Devices and outlet config still show the choice (built-in vs extension by name). No custom extensions: same UI, shorter list — provider/outlet dropdown still shown with only built-ins.
- **Example plugin and CI:** Kind: outlet plugin — minimal custom outlet (e.g. no-op or log-only) implementing the outlet contract (EXT-02). How "real": planner decides. CI: build + end-to-end — runtime runs with the example outlet; assert something observable. Where example lives: planner decides (workspace member vs separate directory).
- **What "still integrate" means:** Control, Hub, observability, runtime (headless): full parity with built-ins (snapshot, strip, visualization, config, Run, integrity, session recording, logs; no special modes or limitations for custom components).
- **Discovery and identity:** Where runtime looks for extensions: planner decides (e.g. fixed dir, configurable paths, default + override). Each extension identified by name only (string in manifest); version is not part of the ID. Duplicate names: fail — invalid config/discovery; do not start or load that extension. Rescan: startup + on demand (explicit refresh via Hub/CLI).

### Claude's Discretion

- None explicitly listed; "planner decides" applies to: example plugin exact placement and minimal vs slightly realistic behavior; discovery strategy (fixed dir vs configurable paths vs default + override).

### Deferred Ideas (OUT OF SCOPE)

- Version in extension ID / selection (v1: name-only; version as metadata for display or compatibility can be added later if needed). Plugin discovery/lifecycle fully specified and documented is EXT-04 (v2); v1 implements discovery and refresh per above.

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| COMP-06 | User can replace any pipeline component (acquisition, signal preprocessing, decoder, output) with a custom/third-party implementation conforming to published contract; rest of pipeline, control, Hub, runtime, observability still integrate | Contracts per slot in neurohid-types/docs; config selection by name; discovery + registry; fail clearly on load/run; apply on next slot start |
| EXT-01 | New device backends addable without changing core orchestration (trait-based or plugin contract) | DeviceProvider trait already exists; extend create_provider to resolve by name from registry; discovery loads dylib or subprocess that implements same trait/contract |
| EXT-02 | New action/output types (e.g. game input, MIDI) addable via defined outlet/effector contract | Define outlet/effector trait in neurohid-types (or docs); OutletTask replaced by trait object or wrapper; config selects built-in vs extension by name |
| EXT-03 | One example plugin (device or outlet) exists and is tested in CI | Example outlet plugin (minimal); CI: build + e2e (runtime runs with example outlet; assert observable behavior); placement: planner decides |

</phase_requirements>

## Standard Stack

### Core

| Library / approach | Version / note | Purpose | Why standard |
|--------------------|----------------|---------|--------------|
| neurohid-types | (workspace) | Contract types (config, control, signal, action) | Already the shared-types crate; contracts live here or in published docs |
| DeviceProvider trait | neurohid-device | Device discovery/connect/stream | Already used; EXT-01 extends via name-based resolution |
| libloading | 0.8.x | Load dynamic libraries (`.so`/`.dll`/`.dylib`) | De facto standard for safe Rust dylib loading; cross-platform |
| serde (manifest) | (workspace) | Extension manifest (name, entry point) | Already used for config; no new dependency |

### Supporting

| Library / approach | Purpose | When to use |
|--------------------|---------|-------------|
| tokio::process or subprocess crate | Spawn and communicate with subprocess plugins | When implementation is Python or external binary (CONTEXT allows both in-process and subprocess) |
| Line-based or JSON over stdin/stdout | Contract for subprocess plugins | Out-of-process plugin protocol; timeout and backpressure are design concerns |

### Alternatives considered

| Instead of | Could use | Tradeoff |
|------------|-----------|----------|
| libloading | dynamic_plugin crate | dynamic_plugin is higher-level but uses C-compatible FFI; for Rust-only dylibs, libloading + exported symbol is simpler |
| In-process only | Subprocess only | CONTEXT mandates both allowed; support both from day one or phase subprocess after dylib |

**Installation:** No new mandatory dependency for v1 if example is in-process Rust; add `libloading` to the crate that performs discovery/load (likely `neurohid-core` or a small `neurohid-plugin` crate). Subprocess: `tokio` already in use; optional `subprocess` if blocking communicate is needed.

## Architecture Patterns

### Recommended project structure (extensions)

- Contracts: `neurohid-types` (or `docs/` for protocol-only contracts) — types/traits that extensions must implement.
- Discovery/registry: in `neurohid-core` or a dedicated `neurohid-plugin` crate — scan directories, parse manifests, enforce name uniqueness, expose "list extensions" for Hub/CLI.
- Factory layer: in `neurohid-core` — for each slot, resolve selection (enum or string name) to concrete implementation (built-in or loaded extension); fail clearly on unknown name or load error.
- Example plugin: workspace member (e.g. `crates/neurohid-outlet-example`) or directory under `.planning`/`examples` — minimal outlet implementing the outlet contract; built and tested in CI.

### Pattern 1: Trait-based slot with name-based selection

**What:** Each pipeline slot has a published trait (or equivalent contract). Config carries either a built-in enum variant or an extension name (string). A factory in core maps that to `Box<dyn Trait>` (or subprocess handle that satisfies the same contract).

**When to use:** For all four slots (acquisition → DeviceProvider; signal → TBD trait; decoder → TBD trait; output → outlet/effector trait).

**Example (existing device pattern):**

```rust
// neurohid-core/src/tasks/device.rs (existing)
async fn create_provider(config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    match config.backend {
        DeviceBackend::Mock => Ok(Box::new(MockProvider::new(...))),
        DeviceBackend::Lsl => create_lsl_provider(config),
        // ... add: DeviceBackend::Extension(name) or separate field device.extension = Some("my-plugin")
        // => registry.load_device_provider(&name).await
    }
}
```

### Pattern 2: Extension manifest (name-only ID)

**What:** Each extension provides a manifest (e.g. `manifest.json` or embedded in binary) with at least a string `name`. Registry discovers extensions from a configured path list, deduplicates by name, and fails at startup if duplicates exist.

**When to use:** For both device and outlet (and future decoder/signal) extensions.

### Pattern 3: Outlet contract (to be defined)

**What:** Define an outlet/effector trait that receives the same inputs the current `OutletTask` uses: sample, feature, action, marker streams (or a single consolidated channel). Implementations can be built-in (current LSL/TCP) or loaded by name. Core spawns one task per outlet instance; control/snapshot show outlet by name and status.

**When to use:** EXT-02; example plugin implements this trait.

### Anti-patterns to avoid

- **Silent fallback when extension fails:** CONTEXT requires fail clearly; never substitute another component.
- **Version as part of ID in v1:** Deferred; use name only.
- **Hub "save" separate from selection:** Choice in Hub must update config immediately (sync).
- **Special UI for "no extensions":** Same UI, shorter list when none are installed.

## Don't Hand-Roll

| Problem | Don't build | Use instead | Why |
|---------|-------------|-------------|-----|
| Dynamic library loading | Manual dlopen/dlsym | libloading | Correct lifetime and symbol handling; cross-platform |
| Subprocess I/O | Raw pipes + manual framing | tokio::process + line/JSON protocol or subprocess crate | Deadlock and timeout handling |
| Manifest parsing | Ad-hoc parsing | serde (JSON/TOML) | Consistency with rest of config |
| ABI-safe plugin API | Exposing complex Rust types across dylib | Trait + C-compatible shim or same compiler/toolchain for in-process | ABI stability is hard; same toolchain or FFI boundary |

**Key insight:** Plugin boundaries are where ABI and lifecycle bugs appear. Use a single, well-documented contract (trait or protocol) and one loading path (dylib or subprocess) per extension type to reduce matrix.

## Common Pitfalls

### Pitfall 1: ABI instability (dylib)

**What goes wrong:** Plugin built with different Rust version or std layout breaks at runtime.
**Why:** Rust does not guarantee ABI stability across compiler versions.
**How to avoid:** Document that in-process plugins must be built with the same toolchain as the host; or use a C-compatible FFI layer and dynamic_plugin-style interface. For v1, same workspace/toolchain for example plugin avoids this.
**Warning signs:** Load succeeds but crashes on first call; or symbol not found.

### Pitfall 2: Duplicate extension names

**What goes wrong:** Two plugins declare the same name; registry or config picks one arbitrarily and behavior is confusing.
**Why:** CONTEXT requires name-only ID; duplicates must be an error.
**How to avoid:** At discovery time, collect by name; if any name appears more than once, fail startup and report which name is duplicated.
**Warning signs:** User sees two entries with same name in Hub; or only one appears and the other is silently dropped.

### Pitfall 3: Silent fallback on load failure

**What goes wrong:** Extension fails to load and runtime falls back to built-in or skips the slot without telling the user.
**Why:** CONTEXT forbids silent fallback.
**How to avoid:** On load/run failure, set pipeline to failed/degraded with explicit reason; surface in snapshot, Hub, and logs.
**Warning signs:** Config says "use plugin X" but runtime behaves as if X is not there.

### Pitfall 4: Config change not applied until full restart

**What goes wrong:** User changes device or outlet in Hub; pipeline keeps old component until full app restart.
**Why:** CONTEXT says apply on next start of that slot (e.g. reconnect device / reload outlet) where design allows.
**How to avoid:** For device: "reconnect" or "rescan" already exists; for outlet: support reloading outlet task when outlet config changes (same pattern as set_signal_config).
**Warning signs:** User has to restart the whole runtime to pick a new outlet.

## Code Examples

### Existing device provider factory (extend for name-based)

```rust
// neurohid-core/src/tasks/device.rs (simplified)
async fn create_provider(config: &DeviceConfig) -> Result<Box<dyn DeviceProvider>> {
    match &config.backend {
        DeviceBackend::Mock => Ok(Box::new(MockProvider::new(MockDeviceConfig::default()))),
        DeviceBackend::Lsl => create_lsl_provider(config),
        DeviceBackend::Serial => create_serial_provider(config),
        DeviceBackend::BrainFlow => create_brainflow_provider(config),
        DeviceBackend::Auto => { /* ... */ }
        // New: resolve extension by name from registry; return Box<dyn DeviceProvider>
        // or fail with clear error if name unknown or load fails
    }
}
```

### libloading (in-process plugin)

```rust
// Conceptual: load a cdylib and retrieve a factory symbol
unsafe {
    let lib = libloading::Library::new(path_to_cdylib)?;
    let create: libloading::Symbol<unsafe extern "C" fn() -> *mut std::ffi::c_void> = lib.get(b"neurohid_outlet_create")?;
    // Wrap in a type that implements Outlet trait and holds Library guard
}
```

### Outlet contract (to implement)

Current `OutletTask` takes `OutletConfig` and broadcast receivers for sample, feature, action, marker; it runs a loop and pushes to LSL/TCP. The contract should be: "given config and these four channels, run until shutdown." Trait can be:

- `async fn run(self, config, sample_rx, feature_rx, action_rx, marker_rx, shutdown) -> Result<()>`  
or a builder that returns a handle. Exact signature is for the planner to align with existing `OutletTask::new(...).run(shutdown).await`.

## State of the Art

| Old approach | Current approach | When changed | Impact |
|--------------|------------------|--------------|--------|
| Hardcoded backends only | Trait-based DeviceProvider + enum selection | Already in codebase | Phase 6 adds name-based extension selection and discovery |
| Single outlet implementation | Outlet as trait + built-in + example plugin | Phase 6 | Enables EXT-02 and EXT-03 |

**Deprecated/outdated:** None specific to this phase. Avoid adding version to extension ID in v1 (deferred).

## Open Questions

1. **Discovery path strategy**  
   - What we know: CONTEXT says planner decides (fixed dir, configurable paths, default + override).  
   - What's unclear: Exact default path(s) (e.g. `~/.neurohid/extensions` vs project-relative).  
   - Recommendation: Planner to define one default and document override via config/env.

2. **Example plugin placement**  
   - What we know: CONTEXT says planner decides workspace member vs separate directory; CI must build + e2e.  
   - What's unclear: Whether example lives in `crates/` as a workspace member or in `examples/` / `.planning` for CI/docs clarity.  
   - Recommendation: Workspace member (e.g. `neurohid-outlet-example`) keeps toolchain and ABI aligned and simplifies CI.

3. **Signal and decoder contracts**  
   - What we know: All four slots must be replaceable (COMP-06); device and outlet are specified first (EXT-01, EXT-02, EXT-03).  
   - What's unclear: Whether Phase 6 delivers all four contracts or only device + outlet with decoder/signal as follow-up.  
   - Recommendation: Planner to scope: at minimum device + outlet contracts and example outlet; signal and decoder contracts can be added in same phase if time allows, or documented as future work.

## Sources

### Primary (HIGH confidence)

- Codebase: `crates/neurohid-device/src/traits.rs` (DeviceProvider, Device); `crates/neurohid-core/src/tasks/device.rs` (create_provider); `crates/neurohid-core/src/tasks/outlet.rs` (OutletTask); `crates/neurohid-types/src/config.rs` (DeviceConfig, OutletConfig).
- Codebase: `docs/architecture-rust-core.md`, `docs/crate-boundaries.md` (placement, neurohid-types as shared types).

### Secondary (MEDIUM confidence)

- libloading docs (docs.rs): safe dylib loading, symbol lifetime.
- Web search: Rust plugin patterns, subprocess JSON contract (subprocess crate, tokio::process).

### Tertiary (LOW confidence)

- dynamic_plugin crate (alternative for C-ABI plugins); external process plugin patterns (struckdown) — not verified in project.

## Metadata

**Confidence breakdown:**

- Standard stack: MEDIUM — libloading and trait pattern are standard; subprocess protocol and exact contract shapes are project-specific and partially TBD.
- Architecture: MEDIUM — pattern (trait + registry + config by name) is clear; discovery path and decoder/signal contract scope left to planner.
- Pitfalls: HIGH — ABI, duplicate names, no silent fallback, apply-on-slot-restart are well specified by CONTEXT and codebase.

**Research date:** 2026-02-21  
**Valid until:** ~30 days; revisit if plugin loading or discovery design changes.
