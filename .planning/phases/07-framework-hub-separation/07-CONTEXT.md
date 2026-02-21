# Phase 7: Framework–Hub Separation - Context

**Gathered:** 2026-02-21
**Status:** Ready for planning

## Phase Boundary

Developer has a clear boundary between the framework (what to depend on) and the Hub (one application built on top). Same repo and binaries; dependency graph and docs define the boundary; dependency audit or CI check enforces it. Requirements: FRAME-01, FRAME-02, FRAME-03, FRAME-04. Scope is fixed; this context clarifies *how* to implement that.

## Implementation Decisions

### Framework surface

- **Mental model:** Framework is like PyTorch or scikit-learn — composable building blocks developers use to build various things. The documented "framework surface" is whatever best presents that to developers (which crates/features to depend on, how to use the pieces).
- **Scope of surface:** If it makes sense, all current crates are acceptable to make up the framework surface; the documented surface is the set of crates/APIs embedders and Hub may depend on.
- **Presentation:** How to name and present the "pieces" (crate-level list vs single entry + conceptual map) is left to research/planning so it best serves the composable-building-blocks idea.

### Hub's allowed dependencies

- **Meaning of "core":** Researcher/planner choose the minimal allowed set from the current dependency graph (e.g. whether Hub lists neurohid-core only, or core + types, or core + calibration + SDK).
- **neurohid-storage:** Direct dependency allowed — Hub may depend on neurohid-storage directly for config/profile UI.
- **neurohid-ipc:** Direct dependency allowed — Hub may depend on neurohid-ipc directly for control client / IPC.
- **Types from component crates:** Re-export from core — if Hub needs a type that lives only in a component crate, add it to core's public API (or facade) so Hub never depends on that component directly.
- **Rule:** Hub depends only on framework-surface crates (the documented allowlist); no dependency on anything outside that list (e.g. no neurohid-hub self-dep).

### Documentation

- **Primary audience:** Embedders — documentation is written for "I'm building another app; what do I depend on?" Contributors get the same information.
- **Doc location, README/index link, relationship to ARCHITECTURE.md:** Left to researcher/planner based on existing docs layout and discoverability.

### Enforcement

- **Allowlist exceptions:** Fail CI until fixed — no permanent exceptions. If Hub has a disallowed dependency, fix by re-exporting from core or by updating the documented allowlist and code together; CI does not allow a "known exception" list.
- **What the check enforces, treatment of neurohid (binaries crate), when the check runs:** Left to researcher/planner so the rule matches the documented boundary and fits existing CI.

### Claude's Discretion

- Framework surface: exact presentation (crate list vs single entry + conceptual map).
- Hub allowed deps: minimal set that counts as "core" for the allowlist.
- Documentation: where the boundary doc lives, whether README/docs index links to it, how it relates to ARCHITECTURE.md.
- Enforcement: what the CI check actually validates, whether the binaries crate (neurohid) is exempt or follows the same rule, and when the check runs (every PR vs on boundary-doc changes, etc.).

## Specific Ideas

- "Framework is like PyTorch or scikit-learn — you use the framework's pieces to build a bunch of stuff."
- Hub is one application built from those pieces; neurohid-service and neurohid-validate are also applications (framework consumers), not part of the "surface" others depend on.

## Deferred Ideas

- None — discussion stayed within phase scope. (Full framework repo split remains FRAME-05 / later milestone.)

---

*Phase: 07-framework-hub-separation*
*Context gathered: 2026-02-21*
