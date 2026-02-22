# Feature Research: v1.2 Framework Consumable (Prep for Python Bindings)

**Domain:** NeuroHID v1.2 — framework as separate repo or publishable package; Hub consumes as dependency; clear API boundary and release story; prep for Python bindings (no bindings in this milestone)  
**Researched:** 2026-02-22  
**Confidence:** MEDIUM–HIGH (Cargo/SemVer official docs; workspace publish and bindings-prep patterns from multiple sources)

## Scope

This document covers **only** the new feature areas for the v1.2 milestone. Existing capabilities (framework surface doc, Hub allowlist, device/signal/decoder/action pipeline, SDK facade, standalone runtime and Hub, Python ML bridge via IPC) are treated as dependencies where relevant. **Python bindings are out of scope for v1.2**; “prep” means feature boundaries and API design that make a future bindings milestone feasible without rework.

---

## 1. Framework as Separate Repo or Publishable Package

### How it typically works

- **Publishable package:** Framework crates live in the same repo (or a dedicated “framework” subtree). They are published to crates.io (or another registry). Dependencies use **version** requirements (e.g. `neurohid-core = "1.2"`). For local development, Cargo allows `path = "../neurohid-core"` alongside `version = "1.2"`; when you publish, Cargo strips the path and the published crate depends only on the version. Consumers (including Hub) then depend on the framework by version from the registry.
- **Separate repo:** Framework lives in its own repository; Hub (and other apps) live in another repo and depend on the framework via registry (e.g. crates.io) or `git = "..."` with an optional `tag`/`rev`. Same consumption model: version or git ref, no path deps across repos.
- **Monorepo with path deps:** Today Hub uses path deps to framework crates. To “consume as dependency” in the sense of v1.2, Hub must be able to **also** build against the framework as a **versioned** dependency (published or git). That implies: (1) framework crates are publishable (metadata, no path-only deps to non-published crates), and (2) either the same repo publishes framework crates and Hub switches to version deps for release, or the framework is split to another repo and Hub depends on it by version/git.

**Source:** Cargo Book [Specifying Dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html), [Publishing](https://doc.rust-lang.org/cargo/reference/publishing.html); Tweag [workspace publish](https://tweag.io/blog/2025-07-10-cargo-package-workspace/) (path+version dual spec, Cargo 1.90 multi-package publish). Confidence: HIGH for Cargo behavior, MEDIUM for “separate repo” as a chosen layout.

### Table Stakes (Users Expect These)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Framework consumable without building from local path** | Embedders and Hub (when used as a consumer) expect to depend on a version or tag, not a path into this repo. | MEDIUM | Publish framework crates to crates.io (or private registry) or host framework in separate repo; Hub uses `neurohid-* = "x.y"` or `git = "..."`. |
| **Single coherent “framework” unit** | Consumers expect one dependency story (e.g. one or a few crates), not a large set of unrelated crates. | LOW–MEDIUM | Already have: framework surface = types + components + core; SDK is the facade. Publish at least the surface crates (or a single umbrella crate) so Hub/apps depend on “the framework” in a documented way. |
| **No path-only dependencies in published crates** | Published crates cannot refer to `path = "..."`; Cargo replaces path with version when packaging. | LOW | Ensure every framework crate’s dependencies are specified with a `version` (and optional `path` for workspace dev); Cargo handles the rest. |
| **Publishable metadata** | crates.io (and embedders) expect description, license, repository, readme. | LOW | Add/fill in Cargo.toml fields for each publishable crate. |

### Differentiators (Competitive Advantage)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Coordinated multi-crate publish** | Release framework as a set in dependency order without manual version bump ordering. | MEDIUM | Cargo 1.90+ `cargo publish` in workspace (or tools like cargo-release, release-plz) can publish multiple packages; otherwise publish in dependency order (types → components → core → SDK). |
| **Choice of layout: same repo vs separate repo** | Same repo = simpler CI and cross-changes; separate repo = strict version boundary and independent release. | MEDIUM | v1.2 can deliver “publishable package” in same repo first; “separate repo” is an optional layout that still consumes the same published versions. |
| **Hub buildable with path or version** | Devs can use path deps in monorepo; CI or external consumers use version deps. | LOW–MEDIUM | Support both: e.g. `neurohid-core = { path = "../neurohid-core", version = "1.2" }` pattern so publish strips path. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Framework as a single giant crate** | “One dep for everything.” | Loses component boundaries, forces all-or-nothing updates, conflicts with existing neurohid-* crates. | Publish the existing surface (types, components, core, optionally SDK); keep crate boundaries. |
| **Only path deps forever** | “We’re always in one repo.” | Hub doesn’t “consume as dependency” in the versioned sense; no release story for the framework. | At least one supported mode where Hub (or another repo) depends by version/git. |
| **Splitting repo without publishing** | “Just move folders.” | If the other repo isn’t published, Hub still has no versioned dependency; release story is unclear. | Publish (or git-tag) the framework so “consumes as dependency” is well-defined. |

---

## 2. Hub Consumes Framework as Dependency

### Expected behavior

- **Dependency form:** Hub’s `Cargo.toml` lists framework crates (e.g. `neurohid-types`, `neurohid-core`, `neurohid-calibration`, `neurohid-storage`, `neurohid-ipc` per allowlist) via **version** (or git), not only path. In monorepo dev, path can be used; for “consumes as dependency,” version (or git) is the canonical form.
- **No reverse coupling:** Framework must not depend on Hub. Already enforced by allowlist and crate-boundaries; remains true when Hub is in another repo or depends by version.
- **Same API surface:** Whether Hub is in the same repo (path) or another (version), it uses the same **framework surface** (allowlist + core facade). So the “clear API boundary” is the same set of crates and public APIs that embedders use.

### Table Stakes (Users Expect These)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Hub depends only on allowlisted framework crates** | Single source of truth for “what Hub may use”; no reaching into non-framework crates. | LOW | Already in `.github/framework-allowlist.toml` and CI; keep when switching to version deps. |
| **Framework crates have compatible versions** | Hub specifies e.g. `neurohid-core = "1.2"`; that version must work with the same major of neurohid-types, etc. | MEDIUM | Publish in dependency order; use shared version or compatible version ranges so a single framework “release” is a set of crate versions that work together. |
| **Clear API boundary** | “What Hub (and any embedder) may call” is documented and stable enough to version. | MEDIUM | docs/framework-surface.md + core facade + allowlist; v1.2 adds release story (see below). |

### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Hub can live in same repo or another** | Flexibility for maintainers: monorepo with path for dev, or split repo with version for clean separation. | MEDIUM | Same framework surface; only dependency source (path vs version/git) and repo layout change. |
| **Binaries (neurohid, neurohid-service, neurohid-validate) still build** | Compatibility constraint from PROJECT.md. | LOW | They depend on Hub and/or core; as long as framework is consumable and Hub consumes it, binaries continue to build. |

### Anti-Features

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Hub depending on internal/component crates outside allowlist** | “Hub needs one more type.” | Breaks boundary; add to core facade or types instead. | Already policy: add to core’s public API so Hub doesn’t depend on component directly. |
| **Tight coupling to “current main”** | “Hub always tracks latest.” | No stable consumption; breaks versioning. | Hub should depend on a released version (or explicit git tag), not an unreleased branch. |

---

## 3. Versioning, Release Cadence, and API Stability

### Expected behavior (ecosystem norms)

- **SemVer:** Cargo and crates.io follow SemVer. For 1.y.z: **major** = incompatible (e.g. remove/rename public items, change layout, add required trait items); **minor** = compatible (e.g. add public items, add optional params with defaults); **patch** = bug fixes that don’t change API. For 0.y.z, the leftmost non-zero component is often treated as “major” (e.g. 0.2 → 0.3 can be breaking).
- **Release cadence:** No single rule. Common: tag and publish on each release; use a changelog (e.g. keep-a-changelog); consider automation (cargo-release, release-plz, cargo-smart-release) for multi-crate workspaces.
- **API stability:** “Clear API boundary” implies the **public** API of the framework surface (types, core facade, allowlisted crates) is what is versioned. Internal refactors that don’t change public items are minor/patch; changing or removing public items is major. Public dependencies (types exposed in your API) should be stable too—a crate can’t reasonably claim 1.0 if it re-exports unstable dependencies.

**Sources:** [Cargo SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html), [Publishing](https://doc.rust-lang.org/cargo/reference/publishing.html) (version bump, changelog, git tag). Confidence: HIGH.

### Table Stakes (Users Expect These)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **SemVer versioning for framework crates** | Embedders and Hub need to express compatible ranges (e.g. `^1.2`). | LOW | Already versioned in Cargo.toml; ensure bumps follow SemVer when publishing. |
| **Changelog and/or release notes** | Users need to know what changed between versions. | LOW–MEDIUM | Per-crate or combined changelog; prefer manually curated. |
| **Git tag per release** | Standard practice for “this commit is 1.2.0.” | LOW | Tag after publish so version is recoverable from source. |
| **Public API = what is stable** | Only the documented framework surface (and its public items) should be considered part of the stability contract. | MEDIUM | Hide or seal internals; avoid exposing internal types in public function signatures so SemVer is tractable. |

### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Coordinated framework version** | One “framework 1.2” = set of crate versions that were tested together. | MEDIUM | Same major version across surface crates, or document “compatible set” (e.g. neurohid-core 1.2.x works with neurohid-types 1.2.x). |
| **Release automation** | Reduces human error in version bumps and publish order. | MEDIUM | cargo-release, release-plz, or Cargo 1.90 workspace publish; optional but valuable. |

### Anti-Features

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Breaking the public API in a minor release** | “Small change.” | Breaks SemVer; consumers’ `^x.y` will pull in breaking changes. | Reserve breaking changes for major; use deprecation first if possible. |
| **No changelog** | “Git history is enough.” | Hard for consumers to judge upgrade risk. | Keep a changelog (even minimal) per release. |

---

## 4. Prep for Python Bindings (Feature Boundaries Only; No Bindings in This Milestone)

### What “prep” implies

- **No bindings in v1.2:** No PyO3/maturin, no `cdylib`, no PyPI package. This milestone only sets up the **Rust side** so a future milestone can add bindings without rework.
- **Bindings-ready design:** Python bindings (e.g. PyO3) work best when the **exposed surface** is small and stable: function-based API, opaque stateful types (handles), simple value types for data in/out. Prep means: (1) the **embedder-facing API** (SDK/core facade) is the same surface that would be bound; (2) no internal-only types leak into that public API; (3) the public API is documented and stable so binding it later doesn’t require redesign.

**Sources:** PyO3/Maturin docs (bindings, abi3); Rust design for FFI (opaque handles, function-based API, minimal surface). Confidence: MEDIUM (design principles; no project-specific verification).

### Table Stakes (Users Expect These)

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Single embedder-facing surface** | Bindings will wrap “the” API; that API should be the same one Rust embedders use (SDK + core facade). | LOW–MEDIUM | Already: framework surface and SDK; ensure there is one clear “surface” (modules/types/functions) that is the candidate for binding. |
| **No internal types in public API** | If the public API returns or takes internal structs, bindings must wrap them or the API must change later. | MEDIUM | Audit core facade and SDK: public functions should take/return types from the facade or types crate, not from component internals. |
| **Documented “what would be bound”** | Future bindings milestone needs a clear list of types and operations. | LOW | Document (e.g. in framework-surface or a short “future bindings” section) the subset of the API that is intended for scripting/bindings. |
| **Stable, versioned API** | Bindings will target a version; that version must be a stable contract. | MEDIUM | Covered by “versioning and API stability” above; prep includes ensuring the bindable surface is part of that contract. |

### Differentiators (Competitive Advantage)

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Opaque handles for stateful objects** | PyO3/FFI patterns prefer “create handle → call functions with handle”; internal state stays in Rust. | MEDIUM | If the current facade exposes stateful types, ensure they are “handle-like” (opaque or minimal) so bindings don’t need to expose internals. |
| **Simple value types for data exchange** | Config, events, results that cross the boundary should be serializable/simple so Python gets copies or simple structs. | LOW–MEDIUM | neurohid-types already has config/control types; ensure any bindable surface uses these or similar, not complex internal types. |

### Anti-Features (Explicitly Out of Scope for v1.2)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Implementing Python bindings in v1.2** | “Prep” sounds like “do bindings.” | PROJECT.md and milestone say no bindings in this milestone. | Prep = API boundary + design only; bindings in a later milestone. |
| **C ABI or cdylib in v1.2** | “Needed for bindings.” | Bindings milestone will add PyO3/cdylib; prep is API design, not build artifact. | Defer cdylib and PyO3 to the bindings milestone. |
| **Exposing every internal type “for flexibility”** | “Python might need everything.” | Huge binding surface, unstable, and leaks internals. | Expose only the documented embedder surface; keep internals internal. |

### Feature boundary summary: “Prep for Python bindings”

- **In scope for v1.2:** Define and stabilize the embedder-facing API (framework surface); ensure it is the single candidate for future binding; avoid leaking internal types; document the intended bindable subset; version that API with SemVer.
- **Out of scope for v1.2:** PyO3, maturin, cdylib, PyPI package, any Python-facing code or build steps.

---

## Feature Dependencies (v1.2)

```text
Framework publishable / separate repo
    ├── depends on: existing framework surface (docs/framework-surface.md, allowlist), publishable metadata
    └── enables: Hub (and others) to depend by version

Hub consumes framework as dependency
    ├── depends on: framework crates published or in separate repo with version/git
    └── depends on: allowlist and no reverse coupling (existing)

Versioning, release cadence, API stability
    ├── depends on: framework surface being clearly defined (existing)
    └── enables: predictable upgrades and “prep for bindings”

Prep for Python bindings (boundaries only)
    ├── depends on: clear API boundary, no internal types in public API, versioned stable surface
    └── enables: future milestone to add PyO3/maturin without redesigning the API
```

### Dependencies on Existing NeuroHID Capabilities

- **docs/framework-surface.md** — Canonical list of framework crates and “what embedders depend on”; v1.2 release story and “consumable” build on this.
- **.github/framework-allowlist.toml** — Hub may depend only on allowlisted crates; when Hub consumes by version, the same set is used.
- **neurohid-sdk** — Public facade; framework “consumable” can be “publish SDK (and its deps)” or “publish surface crates and SDK re-exports them.”
- **neurohid-core** — Orchestration and facade; core’s public API is the main embedder surface; must remain the composition layer and the place to add re-exports so Hub doesn’t reach into components.
- **docs/crate-boundaries.md** — Layer map (types → components → core → hub | sdk | binary); unchanged; release and versioning apply to the published subset.

---

## MVP Definition (v1.2)

### Launch With (v1.2)

- [ ] **Framework as publishable package** — Framework crates (at least surface: types, components, core; optionally SDK) publishable to crates.io (or chosen registry); metadata complete; no path-only deps in published package.
- [ ] **Hub consumes framework as dependency** — Hub can build against the framework via version (or git) dependency; allowlist unchanged; same API surface; binaries (neurohid, neurohid-service, neurohid-validate) still build.
- [ ] **Clear API boundary and release story** — Documented which crates/APIs are stable and versioned; release process (version bump, changelog, tag, publish order or tooling); SemVer followed for public API.
- [ ] **Prep for Python bindings** — Single embedder-facing surface identified and documented; no internal types in that public API (or documented exceptions); short “intended bindable surface” note for future milestone; no bindings or cdylib in v1.2.

### Add After v1.2

- [ ] **Python bindings milestone** — PyO3/maturin, scriptable runtime from Python, PyPI package, tighter ML bridge.
- [ ] **Separate repo (optional)** — Move framework to its own repo; Hub (and others) depend on it by version/git; same published surface.

### Future Consideration

- [ ] **Private registry** — If not publishing to crates.io, host a private registry and document how Hub consumes from it.

---

## Feature Prioritization Matrix (v1.2)

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Framework publishable (metadata, no path-only) | HIGH | LOW–MEDIUM | P1 |
| Hub depends by version (or git) | HIGH | MEDIUM | P1 |
| Release story (changelog, tag, SemVer) | HIGH | LOW–MEDIUM | P1 |
| Clear API boundary (documented, stable) | HIGH | MEDIUM | P1 |
| Prep for bindings (surface only, no internals) | HIGH | MEDIUM | P1 |
| Coordinated multi-crate publish / automation | MEDIUM | MEDIUM | P2 |
| Separate repo layout | MEDIUM | HIGH | P3 |

**Priority key:** P1 = must have for v1.2; P2 = should have; P3 = optional / later.

---

## Sources

- Cargo: [Publishing on crates.io](https://doc.rust-lang.org/cargo/reference/publishing.html), [Specifying dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html), [SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html) (HIGH — official).
- Tweag: [Publish all your crates everywhere (workspace)](https://tweag.io/blog/2025-07-10-cargo-package-workspace/) (MEDIUM — Cargo 1.90 multi-package publish, path+version dual spec).
- Rust API / FFI: Rust design for FFI (opaque handles, function-based API, minimal surface); PyO3/Maturin (bindings, abi3) (MEDIUM — ecosystem patterns).
- NeuroHID: PROJECT.md, docs/framework-surface.md, docs/crate-boundaries.md, .github/framework-allowlist.toml (HIGH — repo).

---
*Feature research for: NeuroHID v1.2 (framework consumable, Hub as consumer, prep for Python bindings)*  
*Researched: 2026-02-22*
