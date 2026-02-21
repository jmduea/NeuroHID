# Phase 7: Framework–Hub Separation - Research

**Researched:** 2026-02-21
**Domain:** Rust workspace boundary design, dependency allowlisting, documentation architecture
**Confidence:** HIGH

## Summary

Phase 7 establishes a clear boundary between the "framework" (composable crates/APIs that embedders and the Hub depend on) and the "Hub" (one application built on that framework). The codebase already follows this direction: `crate-boundaries.md` and `architecture-rust-core.md` define layers; `neurohid-core` exposes a `facade` for IPC/storage; and Hub currently depends only on types, core, calibration, storage, and ipc—no direct device/signal/platform. What is missing is (1) a single documented "framework surface" for embedders, (2) an explicit Hub allowlist and CI enforcement so new direct deps cannot be added by mistake, and (3) docs that frame the boundary for contributors and embedders.

Enforcement cannot be done with cargo-deny alone (it bans crates workspace-wide, not "crate A may not depend on crate B"). Use a small script that consumes `cargo metadata --format-version=1`, filters to the Hub package, and asserts its path dependencies are a subset of the allowlist; run that script in CI and fail on violation. No permanent exceptions: fix by re-exporting from core or updating the allowlist and code together.

**Primary recommendation:** Define the framework surface and Hub allowlist in one canonical doc (e.g. `docs/framework-surface.md` or a dedicated section in `docs/crate-boundaries.md`), add a CI job that runs a dependency-allowlist check for `neurohid-hub` (and optionally `neurohid` binaries) using a script over `cargo metadata`, and link that doc from `docs/index.md` and the README so embedders and contributors can find it.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **Framework surface:** Framework is like PyTorch or scikit-learn — composable building blocks. The documented "framework surface" is whatever best presents that to developers (which crates/features to depend on, how to use the pieces). All current crates are acceptable to make up the framework surface; the documented surface is the set of crates/APIs embedders and Hub may depend on. How to name and present the pieces (crate-level list vs single entry + conceptual map) is left to research/planning.
- **Hub's allowed dependencies:** Researcher/planner choose the minimal allowed set from the current dependency graph. neurohid-storage and neurohid-ipc direct dependency allowed (Hub may depend on them for config/profile UI and control client / IPC). Types from component crates: re-export from core so Hub never depends on that component directly. Rule: Hub depends only on framework-surface crates (documented allowlist); no dependency on anything outside that list (e.g. no neurohid-hub self-dep).
- **Documentation:** Primary audience is embedders — "I'm building another app; what do I depend on?" Contributors get the same information. Doc location, README/index link, relationship to ARCHITECTURE.md left to researcher/planner based on existing docs layout and discoverability.
- **Enforcement:** Allowlist exceptions: fail CI until fixed — no permanent exceptions. If Hub has a disallowed dependency, fix by re-exporting from core or by updating the documented allowlist and code together; CI does not allow a "known exception" list. What the check enforces, treatment of neurohid (binaries crate), and when the check runs are left to researcher/planner.

### Claude's Discretion

- Framework surface: exact presentation (crate list vs single entry + conceptual map).
- Hub allowed deps: minimal set that counts as "core" for the allowlist.
- Documentation: where the boundary doc lives, whether README/docs index links to it, how it relates to ARCHITECTURE.md.
- Enforcement: what the CI check actually validates, whether the binaries crate (neurohid) is exempt or follows the same rule, and when the check runs (every PR vs on boundary-doc changes, etc.).

### Deferred Ideas (OUT OF SCOPE)

- None — discussion stayed within phase scope. (Full framework repo split remains FRAME-05 / later milestone.)

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| FRAME-01 | Developer can identify the documented framework surface (which crates and features to depend on) and use it without depending on Hub internals | Single canonical doc (framework-surface.md or crate-boundaries section) listing framework crates/features and entry points; embedder-oriented wording; linked from docs index and README |
| FRAME-02 | Hub is documented as one application built on top of the framework (same binaries; dependency graph and docs define the boundary) | Same doc defines "framework" vs "Hub as one app"; dependency graph reflected in allowlist and layer map in crate-boundaries/architecture |
| FRAME-03 | Hub depends only on core (and calibration) and the framework facade; dependency audit or CI check enforces no disallowed direct deps from Hub to component crates | Allowlist: neurohid-types, neurohid-core, neurohid-calibration, neurohid-storage, neurohid-ipc; CI script using cargo metadata to assert Hub's path deps ⊆ allowlist; no exception list |
| FRAME-04 | Docs describe the framework vs Hub boundary for contributors and embedders | Framework-surface doc (or section) plus index/README link; relationship to crate-boundaries.md and architecture-rust-core.md documented |

</phase_requirements>

## Standard Stack

### Core

| Tool / approach | Purpose | Why standard |
|-----------------|---------|----------------|
| Cargo metadata (--format-version=1) | Query workspace package graph and per-package dependencies | Official Cargo output; no extra deps; scripts can parse JSON to get direct deps per crate |
| Allowlist script (e.g. PowerShell or Python) | Enforce "neurohid-hub may only depend on X, Y, Z" | cargo-deny bans are workspace-wide and cannot express "crate A must not depend on crate B"; script is straightforward and explicit |
| docs/crate-boundaries.md + architecture-rust-core.md | Existing boundary and layer docs | Already canonical; Phase 7 extends or references them rather than replacing |

### Supporting

| Item | Purpose | When to use |
|------|---------|-------------|
| neurohid-core `facade` module | Re-export IPC and storage types for embedders | When Hub or another app needs types that live in component crates; add to core's public API so app does not depend on component directly |
| Single framework-surface doc | One place for "what do I depend on?" | Embedder discovery; can be a new doc or a dedicated section in crate-boundaries |

### Alternatives considered

| Instead of | Could use | Tradeoff |
|------------|-----------|----------|
| Script over cargo metadata | cargo-deny [bans] | cargo-deny bans/allowlists apply to the whole dependency tree, not "package P may not list package Q as direct dep"; script is the right tool for allowlisting Hub's direct deps |
| New doc framework-surface.md | Section in crate-boundaries.md | New doc keeps crate-boundaries focused on placement rules; section keeps one fewer file. Recommend one canonical place (either is fine) and link from index + README |

**Installation:** No new runtime dependencies required. For CI, the check can be a repository script (e.g. `.github/scripts/check-hub-deps.ps1` or `check-framework-boundary.ps1`) that invokes `cargo metadata` and parses the result. Optionally install `cargo-deny` later for license/source checks, but it is not required for Phase 7.

## Architecture patterns

### Recommended project structure (unchanged)

```
crates/
├── neurohid-types      # Shared types (no internal deps)
├── neurohid-device     # Component
├── neurohid-signal     # Component
├── neurohid-platform   # Component
├── neurohid-ipc        # Component (Hub may depend directly)
├── neurohid-storage    # Component (Hub may depend directly)
├── neurohid-calibration# Component (Hub may depend directly)
├── neurohid-core       # Orchestration + facade re-exports
├── neurohid-hub        # One application (depends only on allowlist)
├── neurohid            # Binaries (hub, service, validate)
└── neurohid-sdk        # Feature-gated facade for external embedders
```

### Pattern 1: Framework surface as documented allowlist

**What:** The "framework" is the set of crates and APIs that embedders and the Hub are allowed to depend on. Document it as a list of crates (and optionally features) plus a short conceptual map (e.g. types → components → core; Hub and other apps sit on top).

**When to use:** Whenever defining what "the framework" is for FRAME-01 and FRAME-02. Present as: (1) list of crate names that are part of the framework surface, (2) which of those Hub is allowed to use as direct dependencies (the Hub allowlist), (3) guidance that other types come from `neurohid_core::facade` or core's public API.

### Pattern 2: CI dependency allowlist check

**What:** A script that (1) runs `cargo metadata --format-version=1` (no-deps is optional; direct deps come from workspace members), (2) finds the package with name `neurohid-hub`, (3) reads its `dependencies` (or equivalent from the manifest), (4) filters to path dependencies (workspace crates), (5) checks that every such dependency is in the allowlist, (6) exits with an error and prints disallowed deps if any.

**When to use:** Run on every PR (or when `crates/` or the boundary doc changes) so that adding a direct Hub dependency on e.g. `neurohid-device` fails CI until the allowlist is updated or the dependency is removed/replaced with core re-exports.

**Example (conceptual):**

```powershell
# Pseudocode: get cargo metadata, find neurohid-hub, get direct path deps,
# compare to allowlist = @("neurohid-types","neurohid-core","neurohid-calibration","neurohid-storage","neurohid-ipc")
# If any path dep not in allowlist, write-error and exit 1
```

### Anti-patterns to avoid

- **Allowing "temporary" or permanent exceptions in CI:** CONTEXT forbids this; fix by re-export in core or by updating allowlist and code together.
- **Documenting the boundary only in code comments:** Embedders need a single discoverable doc; link from README and docs/index.md.
- **Using cargo-deny to forbid Hub from depending on neurohid-device:** That would require banning `neurohid-device` for some packages only; cargo-deny bans are workspace-wide, so core (which must depend on neurohid-device) would be affected. Use a script that targets the Hub package instead.

## Don't hand-roll

| Problem | Don't build | Use instead | Why |
|---------|-------------|-------------|-----|
| "Which crates can Hub depend on?" | Ad-hoc review only | Explicit allowlist + script over cargo metadata | Prevents drift; CI enforces the same rule every time |
| "Where do I get type X for my app?" | Guessing or depending on component crate | core facade or core public API | Single stable surface; component crates stay internal to framework |
| Multiple conflicting docs for "framework surface" | Several scattered pages | One canonical doc (or one section) + links | Single source of truth for embedders and contributors |

**Key insight:** The main risk is accidental addition of a direct Hub→component dependency (e.g. neurohid-device). A small, explicit allowlist and a CI check that runs on the dependency graph are the standard way to prevent that without hand-rolling a custom Cargo plugin.

## Common pitfalls

### Pitfall 1: Confusing "framework surface" with "only neurohid-core"

**What goes wrong:** Documenting that embedders must depend only on neurohid-core, while CONTEXT allows Hub to depend directly on neurohid-storage and neurohid-ipc.

**Why it happens:** "Framework facade" is sometimes read as "one crate only." Here, the framework surface is the set of crates/APIs embedders may use; the Hub allowlist is a subset of that (types, core, calibration, storage, ipc).

**How to avoid:** Define "framework surface" as the full set of crates that are part of the framework; define "Hub allowlist" explicitly as the minimal set of workspace crates Hub may list as direct dependencies. Document both.

**Warning signs:** Docs that say "depend only on neurohid-core" without listing the other allowed direct deps for Hub.

### Pitfall 2: Letting the binaries crate (neurohid) bypass the rule

**What goes wrong:** neurohid (which builds the GUI, service, and validate binaries) currently depends on hub, core, ipc, storage, types. If the CI check only runs for neurohid-hub, neurohid could add a direct dep on neurohid-device and the check would pass.

**Why it happens:** The "application" that should be restricted might be interpreted as only the hub library.

**How to avoid:** Decide explicitly: either (a) the check applies only to neurohid-hub (and neurohid is allowed to depend on hub + core + ipc + storage + types as today), or (b) the check applies to both neurohid-hub and neurohid (binaries) with the same allowlist. CONTEXT leaves this to planner; recommend (b) if the intent is "no application crate may depend on component crates directly."

**Warning signs:** CI only checks neurohid-hub while neurohid (binaries) has more direct deps.

### Pitfall 3: Allowlist and doc drifting out of sync

**What goes wrong:** The script allowlist is updated but the framework-surface doc is not (or vice versa), so docs say "Hub may depend on X, Y, Z" and the script allows a different set.

**Why it happens:** Two sources of truth.

**How to avoid:** Keep the allowlist in one place. Option A: script reads the allowlist from a config file or a comment block in the script; doc quotes or links to that. Option B: doc is the source of truth and the script has a comment "allowlist must match docs/framework-surface.md section X." Prefer a single source (e.g. script or a small TOML/JSON that the doc references) so the script and doc cannot diverge.

**Warning signs:** Doc lists five allowed crates and script allows six (or four).

## Code examples

### Getting direct dependencies for a workspace member (conceptual)

`cargo metadata --format-version=1` returns a JSON with a `packages` array. Each package has an `id` and `dependencies` (package IDs). For "direct" path deps of `neurohid-hub`, use the workspace member's manifest: either parse `crates/neurohid-hub/Cargo.toml` or use the metadata `packages` entry for neurohid-hub and collect dependency names that resolve to path dependencies (e.g. `source: null` or workspace member). Then compare to the allowlist.

### Existing facade pattern (neurohid-core)

```rust
// crates/neurohid-core/src/lib.rs (existing)
pub mod facade {
    pub use neurohid_ipc::{IpcClient, IpcConfig, IpcTransport, send_control_request_blocking};
    pub use neurohid_storage::{ConfigStore, DataPaths, ProfileStore, SecureStorage, initialize};
}
```

When Hub (or another app) needs a type that lives only in a component crate, add it to `neurohid_core::facade` (or core's public API) so the app does not add a direct dependency on that component.

## State of the art

| Old approach | Current approach | When changed | Impact |
|--------------|------------------|--------------|--------|
| Hub depending on device/signal directly | Hub depends on core + allowed components (storage, ipc, calibration); device/signal via core | 2025-06-25 (crate-boundaries update notes) | Boundary already partially enforced by convention; Phase 7 makes it explicit and CI-enforced |
| No formal allowlist | Explicit allowlist + CI script | Phase 7 | Prevents accidental new direct deps |

**Deprecated/outdated:** None for this phase. Full framework repo split (FRAME-05) is deferred.

## Open questions

1. **Exact allowlist for neurohid (binaries crate)**  
   - What we know: CONTEXT allows planner to decide whether the binaries crate is subject to the same rule and what the check enforces.  
   - What's unclear: Whether neurohid should be allowed to depend only on neurohid-hub + neurohid-core + neurohid-ipc + neurohid-storage + neurohid-types (current set) and no component crates beyond those.  
   - Recommendation: Treat neurohid (binaries) like Hub: allowlist = same as Hub (types, core, calibration, storage, ipc) plus neurohid-hub for the GUI binary. So the binaries crate must not add direct deps on neurohid-device, neurohid-signal, neurohid-platform.

2. **Doc location: new file vs crate-boundaries section**  
   - What we know: CONTEXT leaves doc location to researcher/planner; discoverability and link from index/README matter.  
   - What's unclear: Whether a new `docs/framework-surface.md` is better than a "Framework surface and Hub boundary" section in `docs/crate-boundaries.md`.  
   - Recommendation: Either is fine. Prefer one canonical place: if crate-boundaries is already long, add `docs/framework-surface.md` and link it from crate-boundaries and index; otherwise extend crate-boundaries with a clear section and link that from index and README.

3. **When the CI check runs**  
   - What we know: CONTEXT says when the check runs is left to planner (every PR vs on boundary-doc changes).  
   - What's unclear: Whether to run on every PR or only when `crates/**/Cargo.toml` or the boundary doc changes.  
   - Recommendation: Run on every PR that touches Rust (e.g. when `determine-impact` sets rust=true), so any change that adds a dependency is checked. Same as other Rust gates; low cost and prevents drift.

## Sources

### Primary (HIGH confidence)

- In-repo: `docs/crate-boundaries.md`, `docs/architecture-rust-core.md`, `crates/neurohid-hub/Cargo.toml`, `crates/neurohid-core/src/lib.rs`, `crates/neurohid/Cargo.toml`, `.github/workflows/ci.yml`
- Cargo: `cargo metadata` output format (packages, dependencies) — standard Cargo behavior

### Secondary (MEDIUM confidence)

- cargo-deny docs (embarkstudios.github.io/cargo-deny): bans apply workspace-wide; no per-package "crate A may not depend on crate B" — verified via search and docs.

### Tertiary (LOW confidence)

- None.

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH — cargo metadata and script-based allowlist are standard; crate-boundaries and architecture already exist and are authoritative.
- Architecture: HIGH — current layer map and dependency direction match CONTEXT; only formalization and CI are new.
- Pitfalls: HIGH — pitfalls follow from CONTEXT (no exceptions, re-export from core, single doc) and from current repo state (Hub vs binaries crate, doc/script sync).

**Research date:** 2026-02-21  
**Valid until:** ~30 days; boundary and CI approach are stable.

---

## RESEARCH COMPLETE
