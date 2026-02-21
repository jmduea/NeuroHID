# Framework Surface and Hub Boundary

This page (`docs/framework-surface.md`) is the canonical framework surface and Hub boundary doc. It answers: **"I'm building another app on NeuroHID — what do I depend on?"** It defines the framework surface (which crates and APIs you may use) and the Hub boundary (Hub is one application built on that framework).

## Mental model

The **framework** is like PyTorch or scikit-learn: composable building blocks you use to build various things. You depend on framework crates and use their public APIs. The **Hub** is one application built on that framework (the desktop GUI). Other applications (e.g. `neurohid-service`, `neurohid-validate`) are also built on the same framework. The dependency graph and this doc define the boundary; CI enforces it.

## Framework surface

The framework surface is the set of crates (and their public APIs) that embedders and the Hub are allowed to depend on. These are the building blocks:

| Layer | Crates | Role |
|-------|--------|------|
| Types | `neurohid-types` | Shared domain/config/control types; no internal deps |
| Components | `neurohid-device`, `neurohid-signal`, `neurohid-platform`, `neurohid-ipc`, `neurohid-storage`, `neurohid-calibration` | Isolated capabilities (EEG backends, signal pipeline, HID, IPC, persistence, calibration) |
| Orchestration | `neurohid-core` | Wires components into end-to-end runtime; exposes facade re-exports |
| Applications | `neurohid-hub`, `neurohid` (binaries), `neurohid-sdk` | Hub = one app; binaries and SDK consume the framework |

This aligns with the layer map in [Crate Boundaries and Placement](crate-boundaries.md): `types → component crates → core → (hub | sdk | binary)`.

## Hub boundary

**Hub is one application.** It may depend only on a fixed allowlist of workspace crates. That allowlist is:

- **Defined in:** [`.github/framework-allowlist.toml`](../.github/framework-allowlist.toml)
- **Enforced by:** CI (a script that asserts `neurohid-hub`'s path dependencies are a subset of the allowlist)

The Hub allowlist is: `neurohid-types`, `neurohid-core`, `neurohid-calibration`, `neurohid-storage`, `neurohid-ipc`. No other workspace path dependencies are allowed. There are no permanent exceptions — if Hub needs a type from a component crate, it is added to `neurohid_core::facade` or core's public API so Hub does not depend on that component directly.

## Guidance for embedders

- **Which crates to depend on:** Use the framework surface crates that match your use case (e.g. types + core for runtime; add calibration/storage/ipc if you need those APIs).
- **Types from component crates:** Prefer `neurohid_core::facade` or core's public API. Do not add a direct dependency on `neurohid-device`, `neurohid-signal`, or `neurohid-platform` from an application crate; use core's re-exports instead.
- **Single source of truth for the allowlist:** The file [`.github/framework-allowlist.toml`](../.github/framework-allowlist.toml) is the canonical allowlist used by CI. This doc and the allowlist file stay in sync (no separate exception lists).

## Conceptual map

```text
  types → components → core → applications
                           ↘
  neurohid-types            neurohid-core → neurohid-hub (one app)
  neurohid-device           neurohid-sdk
  neurohid-signal           neurohid (binaries: neurohid, neurohid-service,
  neurohid-platform              neurohid-validate)
  neurohid-ipc
  neurohid-storage
  neurohid-calibration
```

Dependencies flow downward. Applications (Hub, service, validate) sit on top of the framework; they do not reach through to component crates except via the documented allowlist (for Hub: types, core, calibration, storage, ipc).
