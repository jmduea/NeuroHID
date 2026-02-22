# Architecture Research: Framework Repo Split and Publishable Package

**Domain:** Rust framework delivery model (publishable-in-place vs separate repo); integration with existing NeuroHID architecture and Hub-as-consumer.
**Researched:** 2026-02-22
**Confidence:** HIGH (based on in-repo codebase and planning docs; no external ecosystem dependency)

## Executive Summary

The existing architecture is a layered monorepo: **types → component crates → neurohid-core → applications** (Hub, binaries, SDK). The v1.1 boundary (framework surface + Hub allowlist) is already enforced; v1.2 adds a **delivery model** so the framework can be consumed as a dependency (version or git) instead of only path deps. Integration is **additive**: same component boundaries and data flow; only **dependency source** and **build/release flows** change. No new runtime components are required. A clear **publish order** (types → components → core) and optional **separate repo** layout are defined so Hub (and later Python bindings) have a stable API to depend on.

## Standard Architecture (Current + After Delivery Model)

### System Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  APPLICATIONS (consumers of framework)                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│  neurohid-hub   neurohid (bins)   neurohid-sdk   [future: Python bindings]   │
│       │                │                │                     │              │
│       └────────────────┴────────────────┴─────────────────────┘              │
│                                    │                                          │
│                    dependency: path (today) OR version/git (v1.2)             │
├─────────────────────────────────────────────────────────────────────────────┤
│  FRAMEWORK SURFACE (what gets published or moved to framework repo)          │
├─────────────────────────────────────────────────────────────────────────────┤
│  neurohid-core (orchestration + facade)                                      │
│       │ depends on                                                           │
│  neurohid-types │ neurohid-device │ neurohid-signal │ neurohid-platform │    │
│  neurohid-ipc  │ neurohid-storage │ neurohid-calibration                     │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities (Unchanged)

| Component | Responsibility | Delivery-model impact |
|-----------|----------------|------------------------|
| neurohid-types | Shared domain/config/control types | Must be published first (or in same repo first in publish order). |
| neurohid-device, -signal, -platform, -ipc, -storage, -calibration | Isolated capabilities | Published after types; no API change. |
| neurohid-core | Orchestration, tasks, facade re-exports | Published last among framework crates; stable API is binding prep. |
| neurohid-hub | One application (GUI) | Consumes framework via allowlist only; dependency source switches path → version/git. |
| neurohid (binaries), neurohid-sdk | Entrypoints and embedder facade | Same; SDK already has optional deps—today path, after v1.2 version. |

## Integration with Existing Architecture

### What Stays the Same

- **Layer map:** types → components → core → applications (see `docs/crate-boundaries.md`, `docs/framework-surface.md`).
- **Hub allowlist:** neurohid-types, neurohid-core, neurohid-calibration, neurohid-storage, neurohid-ipc. No new crates on allowlist.
- **Data flow:** Signal → HID and Runtime ↔ Python bridge unchanged. No new tasks or channels.
- **Facade rule:** Hub (and embedders) use `neurohid_core::facade` and core’s public API; they do not depend on neurohid-device, neurohid-signal, neurohid-platform directly.
- **CI boundary:** check-hub-deps (allowlist) remains; can be extended to “Hub must build with framework from version/git” when not using path deps.

### What Changes (New or Modified)

| Area | Current | After (publishable-in-place) | After (separate repo) |
|------|---------|------------------------------|------------------------|
| **Dependency source (Hub, SDK, binaries)** | Path only (`path = "../neurohid-core"` etc.) | Version from workspace or crates.io (e.g. `neurohid-core = { version = "0.1" }`) | Git dependency or crates.io (e.g. `neurohid-core = { git = "https://github.com/.../neurohid-framework", tag = "v0.1.0" }`) |
| **Framework crates publish** | All `publish = false` except neurohid, neurohid-sdk | Framework crates `publish = true`; versions in dependency order | Framework repo publishes same set (or single umbrella crate) |
| **Build/release pipeline** | Single workspace build; publish workflow only runs for neurohid + neurohid-sdk (SDK’s path deps make full publish invalid for crates.io) | New: publish workflow publishes framework crates in order (types → components → core), then applications | New: framework repo has its own version + publish; main repo CI depends on framework ref |
| **Repo boundaries** | One repo | One repo, framework = subset of crates | Two repos: framework repo (types + components + core), application repo (Hub, binaries, SDK, Python) or monorepo that pulls framework as dep |

### Integration Points (Explicit)

| Integration point | Role in v1.2 |
|-------------------|--------------|
| **Hub `Cargo.toml`** | Switch path deps to version (or git) for framework allowlist crates; or keep path for dev and use version in release. |
| **neurohid-core public API and `facade`** | Becomes the **stable API surface** for “framework” and prep for Python bindings; avoid breaking changes or document them. |
| **neurohid-sdk `Cargo.toml`** | Optional deps today are path; switch to version deps once framework crates are published so `cargo publish -p neurohid-sdk` is valid. |
| **`.github/framework-allowlist.toml`** | Unchanged; still lists which crates Hub may depend on (same set, different source). |
| **CI: check-hub-deps** | Still enforce allowlist; add optional job “build Hub against published/git framework” if using version/git. |
| **CI: publish-crates** | Extend to publish framework crates in dependency order before neurohid-sdk and neurohid. |
| **Python bindings (future)** | No new components this milestone; core’s stable API and types are the boundary for a future FFI layer. |

## Recommended Options: Publishable-in-Place vs Separate Repo

### Option A: Publishable-in-Place (same repo)

**What:** Keep all crates in the current workspace. Set `publish = true` and coherent versions for the framework set. Hub and binaries depend on framework via **version** (workspace or crates.io). Build order is unchanged; release flow adds a **publish order** for crates.io.

**Pros:** No repo split; single history and CI; easy to keep docs and code in sync.  
**Cons:** Monorepo stays large; release process must respect dependency order.

**Build order (for publish):**

1. neurohid-types  
2. neurohid-device, neurohid-signal, neurohid-platform, neurohid-ipc, neurohid-storage, neurohid-calibration (no inter-deps among these; can parallelize)  
3. neurohid-core  
4. neurohid-hub, neurohid, neurohid-sdk (applications; SDK and neurohid binary need framework published first if they use version deps)

### Option B: Separate Repo (framework repo)

**What:** New repo containing only framework crates (types + components + core). Main repo (Hub, binaries, SDK, Python) depends on framework via **git tag** or **crates.io**. Same layer map and API; dependency source is external.

**Pros:** Clear ownership boundary; framework can version and release independently; smaller application repo.  
**Cons:** Cross-repo changes and version bumps; CI in main repo must pin framework ref; docs may live in two places.

**Build order:** Inside framework repo: same as above (types → components → core). Main repo build: fetch framework dependency (git/crates.io), then build Hub/binaries/SDK.

## Recommended Project Structure (Per Option)

### Publishable-in-Place (no structural change)

```
neurohid/                    # same repo
├── crates/
│   ├── neurohid-types       # publish = true, first in order
│   ├── neurohid-device      # publish = true
│   ├── neurohid-signal      # publish = true
│   ├── neurohid-platform    # publish = true
│   ├── neurohid-ipc         # publish = true
│   ├── neurohid-storage     # publish = true
│   ├── neurohid-calibration # publish = true
│   ├── neurohid-core        # publish = true
│   ├── neurohid-hub         # publish = false; deps = version or path
│   ├── neurohid             # publish = true; deps = version or path
│   ├── neurohid-sdk         # publish = true; optional deps = version
│   └── neurohid-outlet-example
├── python/
└── .github/workflows        # publish-crates: publish framework then apps
```

### Separate Repo (framework repo layout)

```
neurohid-framework/          # new repo
├── crates/
│   ├── neurohid-types
│   ├── neurohid-device
│   ├── neurohid-signal
│   ├── neurohid-platform
│   ├── neurohid-ipc
│   ├── neurohid-storage
│   ├── neurohid-calibration
│   └── neurohid-core
├── Cargo.toml               # workspace with only these members
└── .github/workflows        # version bump + publish to crates.io

neurohid/                    # existing repo (Hub, apps, Python)
├── crates/
│   ├── neurohid-hub         # dependency: neurohid-core = { git = "..." }
│   ├── neurohid
│   ├── neurohid-sdk
│   └── ...
├── python/
└── Cargo.toml               # workspace; framework deps from git/crates.io
```

## Data Flow (Unchanged)

- **Request/control flow:** Hub or CLI → ControlRequest (IPC) → runtime → ControlSnapshot. No change when Hub consumes framework as dependency.
- **Signal → HID:** DeviceTask → SignalTask → DecoderTask → ActionTask → OutletTask (unchanged).
- **State:** Config/profile in neurohid-storage; runtime and Hub state as today. No new data flows introduced by the delivery model.

## Build Order and Phases for Implementation

### Phase 1: Publishable-in-place (recommended first)

1. **Enable publish and versions**  
   Set `publish = true` and consistent versions for: neurohid-types, neurohid-device, neurohid-signal, neurohid-platform, neurohid-ipc, neurohid-storage, neurohid-calibration, neurohid-core. Ensure `[workspace.package]` version/repository is correct.

2. **Publish workflow (dependency order)**  
   Extend `.github/workflows/publish-crates.yml` to publish framework crates in order: types first, then the six components (parallel if no inter-deps), then core. Then publish neurohid-sdk and neurohid (so they can depend on published framework).

3. **Hub and binaries: dependency source**  
   - **Option 3a:** Keep path deps for local dev; use version deps only in release builds or in a separate “release” Cargo.toml profile.  
   - **Option 3b:** Switch Hub and binaries to version deps (e.g. workspace version) so that `cargo build` in the repo still works (workspace dependency resolution).  
   Prefer 3b for simplicity: e.g. `neurohid-core = { version = "0.1", path = "../neurohid-core" }` (path overrides version when present) or workspace version only.

4. **SDK**  
   Change optional dependencies from `path = "..."` to `version = "0.1"` (or workspace) so `cargo publish -p neurohid-sdk` is valid for crates.io.

5. **CI**  
   Add or adjust a job that verifies “Hub builds with framework as crates.io (or workspace) dependency” and keep check-hub-deps (allowlist) as-is.

### Phase 2: Optional separate repo

1. **Extract framework**  
   Create neurohid-framework repo; copy or move types + 6 components + core; minimal Cargo.toml workspace; no Hub, no binaries, no Python.

2. **Version and publish from framework repo**  
   Establish versioning (e.g. 0.1.0) and a workflow to publish to crates.io in the same dependency order.

3. **Main repo consumes framework**  
   In neurohid (main repo), replace path deps for framework crates with git (e.g. `tag = "v0.1.0"`) or crates.io version. Hub allowlist remains the same set of crate names; source is git/crates.io.

4. **CI in main repo**  
   Pin framework ref (tag or version); run check-hub-deps and full build against that ref.

### Phase ordering rationale

- **Publishable-in-place first:** Delivers “Hub consumes framework as dependency” and “clear release story” without repo split; validates publish order and versioning in one repo.  
- **Separate repo second (if desired):** Adds a clean repo boundary and independent framework releases; can be done after publish order and versioning are stable.

## Anti-Patterns to Avoid

### Reverse or circular dependencies

**What:** Core depending on Hub, or a component depending on core.  
**Why bad:** Layer map and allowlist assume types → components → core → applications.  
**Do instead:** Keep dependency direction; add new APIs in core/facade or types when Hub needs something from a component.

### Hub depending outside the allowlist

**What:** Adding neurohid-device, neurohid-signal, or neurohid-platform as direct Hub deps.  
**Why bad:** Breaks framework boundary and CI.  
**Do instead:** Expose types/APIs via neurohid-core (or facade) and keep Hub allowlist unchanged.

### Publishing out of order

**What:** Publishing neurohid-core before neurohid-types or a component.  
**Why bad:** crates.io publish will fail (missing dependency).  
**Do instead:** Publish in dependency order: types → components → core → applications.

### Unversioned or unstable “framework” API for bindings

**What:** Changing core’s public API or facade without regard for future FFI.  
**Why bad:** Python bindings will target this surface; churn forces binding rewrites.  
**Do instead:** Treat neurohid-core’s public API and facade as stable; document and avoid breaking changes (or use semver and explicit minor bumps).

## Scalability Considerations

| Concern | Publishable-in-place | Separate repo |
|---------|----------------------|---------------|
| Many framework contributors | Single PR flow; version bumps in same repo | Framework repo can have its own PR/merge flow |
| Release cadence | One release workflow; framework and apps versioned together or in lockstep | Framework can release independently; main repo pins ref |
| Python bindings (future) | Single repo for Rust + Python; bindings depend on published core/types | Bindings in main repo depend on published or git framework |

## Summary: New vs Modified Components

| Item | New? | Description |
|------|------|-------------|
| Framework as a *delivery unit* | Yes (concept) | The set of crates (types + components + core) with a release story; no new code crate. |
| Umbrella crate (e.g. neurohid-framework) | Optional | Single crate that re-exports the surface for one-line dependency; convenience only. |
| Repo “neurohid-framework” | Optional | New repo only if choosing separate-repo; same crates as today. |
| Hub Cargo.toml | Modified | Dependency source path → version or git. |
| neurohid-sdk Cargo.toml | Modified | Optional deps path → version. |
| publish-crates workflow | Modified | Publish framework crates in order, then apps. |
| check-hub-deps / CI | Unchanged or extended | Allowlist unchanged; optional “build against published/git framework” job. |
| neurohid-core API / facade | Unchanged (behavior) | Treated as stable for binding prep; no structural change. |

## Sources

- `.planning/PROJECT.md` — v1.2 goal and target features
- `.planning/codebase/ARCHITECTURE.md` — existing layers and data flow
- `.planning/codebase/STRUCTURE.md` — crate layout and placement
- `docs/framework-surface.md` — framework surface and Hub allowlist
- `docs/crate-boundaries.md` — layer map and dependency direction
- `.github/framework-allowlist.toml` — Hub allowlist
- `Cargo.toml` (workspace and crates) — current publish flags and path deps
- `.github/workflows/publish-crates.yml` — current publish scope (neurohid, neurohid-sdk only)

---
*Architecture research for: framework repo split and publishable package (v1.2)*  
*Researched: 2026-02-22*
