# Crate Boundaries and Placement Guide

This page defines where code should live in the Rust workspace and which crates
are allowed to depend on which layers.

## Layer Map (low-level to high-level)

1. `neurohid-types`
   - Shared domain/config/control types only.
   - No runtime orchestration or UI behavior.
2. Runtime component crates
   - `neurohid-device`, `neurohid-signal`, `neurohid-platform`, `neurohid-ipc`,
     `neurohid-storage`
   - Own isolated capabilities and subsystem logic.
3. Composition/orchestration
   - `neurohid-core`
   - Wires component crates into end-to-end runtime behavior.
4. UI and entrypoints
   - `neuroide-hub` (desktop UI)
   - `neuroide` (GUI app)
   - `neurohid` (published library facade/re-export surface)

## Placement Rules

- Put shared schemas/contracts in `neurohid-types`.
- Put signal transformations/features in `neurohid-signal`.
- Put device transport/discovery logic in `neurohid-device`.
- Put OS/HID integration in `neurohid-platform`.
- Put runtime-to-ML transport/protocol client logic in `neurohid-ipc`.
- Put profile/config persistence logic in `neurohid-storage`.
- Put multi-crate runtime coordination in `neurohid-core`.
- Put UI-only behavior and presentation in `neuroide-hub` (including calibration panels and games).
- Put public convenience exports for external Rust users in `neurohid` (the facade crate).

## Framework surface and Hub boundary

The framework surface (which crates and APIs embedders and Hub may depend on) and the Hub allowlist are defined in [framework-surface.md](framework-surface.md). That doc is the source of truth for "what do I depend on?" and for Hub's allowed path dependencies; this document defines placement and the layer map. The Hub allowlist is enforced by CI and defined in [`.github/framework-allowlist.toml`](../.github/framework-allowlist.toml).

## Dependency Direction

Preferred dependency flow:

`types -> component crates -> core -> (hub | facade | app)`

Avoid reverse coupling (for example, component crates depending on `core` or UI crates).

## Change Checklist

When adding or moving code:

- Confirm crate placement against the rules above.
- Prefer adding APIs downward (component crates) and composing upward (`core`).
- Update this document if ownership boundaries or allowed dependencies change.
- Update `README.md` and `docs/index.md` links if navigation changes.

## PR Rationale Template (for Cargo manifest changes)

When a PR changes `Cargo.toml` files, include a short section in
`docs/crate-boundaries.md` update notes using this template:

```md
### <YYYY-MM-DD> <PR/Issue reference>

- Change summary:
- Boundary impact: (none | minor | structural)
- Dependency direction check:
   - [ ] No reverse coupling introduced
   - [ ] Layer map still valid
- Placement rationale:
- Follow-up needed:
```

Keep rationale concise (3-6 bullets) unless crate ownership actually changes.

## Update Notes

### 2026-02-22 BIND-01 API surface audit (v1.2)

- Change summary: Removed `platform` feature re-export from published `neurohid` facade crate; made `ServiceHandle` fields `pub(crate)`; added `#[doc(hidden)]` to `neurohid_core::tasks`. Documented `neurohid-core → neurohid-platform` allowed dependency.
- Boundary impact: minor
- Dependency direction check:
  - [x] No reverse coupling introduced
  - [x] Layer map still valid
- Placement rationale: `neurohid-platform` is an internal OS/HID layer used only inside `neurohid-core::tasks::action`. It is not re-exported by the `neurohid` facade crate (which has `publish = true`) because it is not part of the stable embedder-facing API. The allowed dependency `neurohid-core → neurohid-platform` is intentional — `neurohid-core` consumes the platform HID output layer internally but does not expose any `neurohid-platform` types in its public API surface.
- Follow-up needed: None; BIND-01 audit complete. BIND-02 bindable surface documented in `docs/bindable-surface.md`.

### 2025-06-25 Hub coupling reduction (advanced-workbench-refactor)

- Change summary: Removed `neurohid-device` and `neurohid-signal` from hub deps; moved `neurohid-ipc` to dev-deps (production code uses `neurohid-core::facade` re-exports).
- Boundary impact: minor
- Dependency direction check:
  - [x] No reverse coupling introduced
  - [x] Layer map still valid
- Placement rationale: Hub should depend on `core` for runtime access, not reach through to component crates directly. IPC/storage access via `core::facade` keeps the layer hierarchy clean.
- Follow-up needed: None.
