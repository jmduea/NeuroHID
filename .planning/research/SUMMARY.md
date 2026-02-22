# Project Research Summary

**Project:** NeuroHID  
**Domain:** v1.2 Framework publishable & release; Hub consumes framework as dependency; prep for Python bindings (no bindings in this milestone).  
**Researched:** 2026-02-22  
**Confidence:** HIGH (stack/architecture/pitfalls); MEDIUM–HIGH (features and release-tooling choice).

## Executive Summary

NeuroHID v1.2 is a **delivery-model milestone**: the existing Rust framework (types → components → core) becomes consumable as a versioned dependency instead of only via in-repo path deps. Experts achieve this by making framework crates publishable (path+version deps, publish metadata, SemVer), defining a clear release story (changelog, tags, publish order or automation), and keeping the same API boundary so Hub and future Python bindings depend on a stable surface.

**Recommended approach:** Publish the framework **in-place** (same monorepo) first: set `publish = true` and path+version for all framework-surface crates, publish in topological order (types → components → core → SDK/binaries), and have Hub consume via path+version so the existing allowlist check still applies. Optional separate repo can follow once publish order and versioning are proven. Do **not** add PyO3/maturin or a bindings crate in v1.2; only stabilize and document the embedder-facing API so a future bindings milestone can wrap it without redesign.

**Key risks:** Path-only deps at publish (fix: add `version` to every path dep in publishable crates); wrong publish order or registry delay (fix: topological order + retry/sleep); CI boundary check only enforcing path deps (fix: keep Hub on path+version so current check stays valid, or extend check to all deps by name if Hub moves to version/git). Mitigate by running `cargo publish --dry-run` for every publishable crate in the first phase and by documenting publish order and version policy in the release phase.

## Key Findings

### Recommended Stack

Framework publishing relies on Cargo’s path+version dependency syntax and workspace metadata; release can use automation (release-plz or cargo-release) or, on Rust 1.90+, `cargo publish --workspace`. Python bindings (PyO3/maturin) are out of scope for v1.2; stack research documents them as the intended future stack.

**Core technologies:**
- **path + version in same dep** — Publishable workspace crates must specify both so the published manifest has a registry version; required for framework crates that are published while Hub/others depend on them.
- **workspace.package + publish field** — Shared version/edition/license and explicit `publish = true/false` per crate; framework surface crates get `publish = true` and full metadata (description, repository, license); Hub and binaries stay `publish = false`.
- **SemVer 2.0** — Version bumps follow Cargo semver rules; 0.x.y minor = breaking, patch = compatible; single coordinated version for the framework set is acceptable and common.
- **release-plz or cargo-release** — Version bump, changelog, publish order, git tags; choose one for CI or local release; Cargo 1.90+ optional `cargo publish --workspace` can replace manual order if toolchain is upgraded.
- **Keep a Changelog + git tags** — Human-readable release notes and traceability per release; tie release tool to CHANGELOG.md.

**Do not add this milestone:** PyO3, maturin, cbindgen, or a neurohid-python crate; keep Hub allowlist and do not publish neurohid-hub as a library.

### Expected Features

**Must have (table stakes):**
- Framework consumable without building from local path — embedders and Hub expect version or tag, not a path into the repo.
- Single coherent framework unit — consumers get one dependency story (SDK + surface crates), not a large set of unrelated crates.
- No path-only dependencies in published crates and publishable metadata (description, license, repository) for each publishable crate.
- Hub depends only on allowlisted framework crates; framework crates have compatible versions; clear, documented API boundary.
- SemVer versioning, changelog/release notes, git tag per release; public API = stability contract.
- Prep for Python bindings: single embedder-facing surface, no internal types in that public API, documented “intended bindable surface”; no bindings or cdylib in v1.2.

**Should have (competitive):**
- Coordinated multi-crate publish (dependency order, optional automation).
- Hub buildable with path or version (path+version keeps dev and release consistent).
- Coordinated framework version and release automation.

**Defer (v2+):**
- Python bindings milestone (PyO3/maturin, scriptable runtime from Python, PyPI).
- Optional separate repo for framework.
- Private registry (if not using crates.io).

### Architecture Approach

Existing architecture is a layered monorepo: **types → component crates → neurohid-core → applications** (Hub, binaries, SDK). v1.2 adds a **delivery model** only: same component boundaries and data flow; dependency source and build/release flows change. No new runtime components. Publish order: types → components (parallel where no inter-deps) → core → SDK/binaries. Option A (publishable-in-place) is recommended first; Option B (separate repo) is optional and can follow.

**Major components:**
1. **neurohid-types** — Shared domain/config/control types; published first.
2. **neurohid-device, -signal, -platform, -ipc, -storage, -calibration** — Isolated capabilities; published after types.
3. **neurohid-core** — Orchestration, tasks, facade; published last among framework crates; stable API is binding prep.
4. **neurohid-hub, neurohid (binaries), neurohid-sdk** — Consumers; Hub uses allowlist only; dependency source switches from path-only to path+version or version/git.

### Critical Pitfalls

1. **Path-only dependencies when publishing** — Add `version` (and optionally `path`) to every workspace dep in publishable crates; use `version.workspace = true` where possible; run `cargo publish -p <crate> --dry-run` for each before first publish.
2. **Publish order and registry delay** — Publish in topological order (types → components → core); add delay/retry (e.g. 10–15 s) after each publish so the registry index is updated before dependents publish.
3. **CI boundary check only sees path dependencies** — If Hub ever uses version/git only, the current check (path deps vs allowlist) won’t apply; either keep Hub on path+version (check unchanged) or extend the check to all deps by crate name.
4. **Version skew between path and published** — Single source of truth for framework versions (e.g. `version.workspace = true`); bump all affected framework crates together; document version policy in release automation.
5. **Cyclic dev-dependencies** — Avoid dev-dependency cycles between publishable workspace crates; audit and fix before first publish; dry-run will fail if cycles exist at publish time.
6. **Missing publish metadata** — Add license, description, repository (and readme as needed) to every crate before setting `publish = true`; use workspace inheritance for shared fields.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Make framework publishable (manifests & metadata)

**Rationale:** Nothing else can ship until crates are valid for crates.io (path+version, metadata, no path-only deps).  
**Delivers:** All framework-surface crates have `publish = true`, path+version deps, required metadata; `cargo publish -p <crate> --dry-run` passes for each.  
**Addresses:** Framework consumable without local path, no path-only deps, publishable metadata (FEATURES.md table stakes).  
**Avoids:** Path-only deps at publish (Pitfall 1), missing metadata (Pitfall 6), cyclic dev-deps (Pitfall 5).  
**Verification:** Dry-run in CI for every publishable crate; checklist for metadata and version on path deps.

### Phase 2: Release story (versioning, changelog, publish order)

**Rationale:** Clear release story and SemVer are table stakes; publish order must be defined before automation or first real publish.  
**Delivers:** Version policy (single source, e.g. workspace.package); changelog and git tags; documented publish order (types → components → core → apps); optional release-plz or cargo-release (or Cargo 1.90 workspace publish).  
**Addresses:** SemVer, changelog, git tag per release, coordinated framework version (FEATURES.md).  
**Avoids:** Publish order and registry delay (Pitfall 2), version skew (Pitfall 4).  
**Uses:** release-plz or cargo-release from STACK.md; Keep a Changelog.

### Phase 3: Hub consumes framework as dependency (path+version)

**Rationale:** Hub must be able to depend on the framework by version (or path+version in repo); boundary and API contract stay clear.  
**Delivers:** Hub (and binaries/SDK) depend on framework crates via path+version (or version only if desired); allowlist unchanged; optional CI job that builds Hub against published/git framework.  
**Addresses:** Hub consumes framework as dependency, Hub only allowlisted crates, clear API boundary (FEATURES.md).  
**Avoids:** CI boundary only path deps (Pitfall 3) — by keeping path+version, current check remains valid; if switching to version-only, extend check to all deps by name.  
**Implements:** Architecture “dependency source” change (ARCHITECTURE.md Phase 1 steps 3–5).

### Phase 4: Prep for Python bindings (API boundary only)

**Rationale:** Bindings milestone will wrap the same surface embedders use; v1.2 only stabilizes and documents that surface.  
**Delivers:** Documented single embedder-facing surface; audit so no internal types leak into public API; short “intended bindable surface” note; SemVer applied to that surface.  
**Addresses:** Prep for Python bindings (single surface, no internals in API, documented bindable subset) — FEATURES.md; no bindings/cdylib in v1.2.  
**Avoids:** Unversioned or unstable API for future bindings (ARCHITECTURE.md anti-pattern).  
**No new crates or tooling:** PyO3/maturin deferred.

### Phase 5 (optional): Separate repo for framework

**Rationale:** Optional after publishable-in-place is stable; adds clean repo boundary and independent framework releases.  
**Delivers:** Framework repo with same crates; main repo depends on framework via git tag or crates.io; CI pins framework ref.  
**Addresses:** “Choice of layout: same repo vs separate repo” (FEATURES.md differentiator).  
**Defer unless required:** P3 in feature prioritization.

### Phase ordering rationale

- Phase 1 must come first: publishable manifests and metadata are the gate for any publish or consumer.
- Phase 2 can overlap with Phase 1 (version policy and changelog) but publish order and automation matter as soon as the first real publish runs.
- Phase 3 depends on Phase 1 (and ideally Phase 2) so Hub can depend on versioned framework; path+version keeps allowlist check valid without CI changes.
- Phase 4 is largely documentation and API audit; can run in parallel with Phase 2/3 once the surface list is fixed.
- Phase 5 is optional and follows after in-place publishing and Hub consumption are proven.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2:** Choice of release-plz vs cargo-release vs Cargo 1.90 `--workspace` and CI integration (release-tooling confidence was MEDIUM in STACK.md).
- **Phase 3:** If moving Hub to version-only (no path), boundary-check extension and allowlist semantics need a small design pass (PITFALLS.md Option B).

Phases with standard patterns (skip research-phase):
- **Phase 1:** Cargo path+version and publish metadata are well documented (Cargo Book, HIGH confidence).
- **Phase 4:** Prep is documentation and API audit only; no new stack (FEATURES.md and ARCHITECTURE.md are clear).

## Confidence Assessment

| Area        | Confidence | Notes                                                                 |
|------------|------------|-----------------------------------------------------------------------|
| Stack      | HIGH       | Cargo Book, path+version, workspace publish; release tool choice MEDIUM. |
| Features   | MEDIUM–HIGH| Cargo/SemVer official; workspace publish and bindings-prep from multiple sources. |
| Architecture | HIGH     | In-repo codebase and planning docs; additive delivery model, no new runtime. |
| Pitfalls   | HIGH       | Cargo/official and documented community gotchas; CI boundary nuance is repo-specific. |

**Overall confidence:** HIGH for “publish framework in-place and Hub consume as dependency”; MEDIUM for “which release tool and exact CI shape” (decide in Phase 2/3).

### Gaps to Address

- **Release tool choice:** release-plz vs cargo-release vs Cargo 1.90 — decide during Phase 2 planning based on CI and maintainer preference.
- **Hub consumption mode:** If main repo keeps path+version, no CI change; if moving to version/git only, extend boundary check and document in the same phase.
- **Rust 1.90:** If upgrading to 1.90+, `cargo publish --workspace` can simplify publish order; otherwise rely on release-plz or cargo-release for order and retry.

## Sources

### Primary (HIGH confidence)

- Cargo Book: [Publishing on crates.io](https://doc.rust-lang.org/cargo/reference/publishing.html), [Specifying Dependencies](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html), [SemVer compatibility](https://doc.rust-lang.org/cargo/reference/semver.html).
- Project: `docs/framework-surface.md`, `docs/crate-boundaries.md`, `.github/framework-allowlist.toml`, `.planning/PROJECT.md`, `.planning/codebase/ARCHITECTURE.md`.

### Secondary (MEDIUM confidence)

- Tweag: [Publish all your crates everywhere (Cargo 1.90)](https://tweag.io/blog/2025-07-10-cargo-package-workspace/) — workspace publish, path+version.
- Gotchas: [Publish Rust crates in a workspace](https://blog.iany.me/2020/10/gotchas-to-publish-rust-crates-in-a-workspace/) — version on path deps, publish order, delay, cyclic dev-deps.
- Release tooling: release-plz config, cargo-release; Maturin (for future bindings).

### Tertiary (reference)

- Cargo issues #1169, #4242 (publish order, cyclic dev-deps); Lindera #358 (workspace.dependencies); project scripts: `check-framework-boundary.ps1`.

---
*Research completed: 2026-02-22*  
*Ready for roadmap: yes*
