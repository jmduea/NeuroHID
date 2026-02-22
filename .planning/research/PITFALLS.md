# Pitfalls Research: Framework Repo Split / Publishable Package in NeuroHID Monorepo

**Domain:** Adding a framework repo split or publishable package to an existing Rust application monorepo; integration with Hub, CI, versioning, and path vs published dependencies.

**Researched:** 2026-02-22  
**Confidence:** HIGH (Cargo/official and documented patterns); MEDIUM where only community posts apply.

---

## Critical Pitfalls

### Pitfall 1: Path-only dependencies when publishing

**What goes wrong:**  
`cargo publish` (or `cargo publish --dry-run`) fails with: *"all dependencies must have a version specified when publishing"* / *"dependency \<crate-name\> does not specify a version"*. Published manifests have path stripped; Cargo requires a version so consumers can resolve the dependency from the registry.

**Why it happens:**  
Workspace members today use only `path = "../..."` for in-repo crates. That is valid for local and CI builds but invalid for packaging to crates.io.

**How to avoid:**  
For every dependency that is a workspace member and that will be published (or that the current crate depends on for publish), specify **both** `path` and `version` in the same dependency entry, e.g.  
`neurohid-core = { path = "../neurohid-core", version = "0.2.0" }`.  
Use `version.workspace = true` in publishable crates and a single source of truth in root `[workspace.package]` (or per-crate) so path and version stay in sync.

**Warning signs:**  
- Any `cargo publish -p <crate> --dry-run` fails with "version" or "dependency" errors.  
- Grep for `path = "\.\./` in publishable crates without a `version` key in the same dependency table entry.

**Phase to address:**  
First phase that introduces publishable framework crates (manifest/versioning phase). Add version to all workspace path deps used by publishable crates and run dry-run for each before merging.

---

### Pitfall 2: Publish order and registry availability delay

**What goes wrong:**  
Publishing crate `foo` fails because it depends on `bar`, and either (1) `bar` was not published yet, or (2) `bar` was just published and is not yet visible to the registry index (typically up to ~10 seconds). Result: failed publish step or flaky release automation.

**Why it happens:**  
Cargo has no built-in "publish all workspace members in order." Publishing is per-crate; dependency resolution at publish time uses the registry, so dependents must be published and visible before dependers.

**How to avoid:**  
(1) Publish in **topological order** (dependency graph: leaves first, then dependers).  
(2) After each publish, **wait and retry** resolution (e.g. 10–15 s and a few retries) before publishing the next crate.  
(3) Use or mirror the behavior of release automation (e.g. `cargo-release`, `cargo-smart-release`, or a script that topologically sorts and publishes with retries).

**Warning signs:**  
- Single job that runs `cargo publish -p A` then immediately `cargo publish -p B` where B depends on A.  
- No retry logic after publish when "crate not found" or resolution errors appear.

**Phase to address:**  
Phase that introduces or updates release/publish automation (e.g. "Release story" or "Publish framework" phase). Define and test publish order and add delay/retry for the registry.

---

### Pitfall 3: CI boundary check only sees path dependencies

**What goes wrong:**  
The current framework-boundary check (`.github/scripts/check-framework-boundary.ps1`) uses `cargo metadata` and restricts **path** dependencies of `neurohid-hub` and `neurohid` (binaries) to the allowlist. If Hub (or binaries) ever consume the framework via **version** (registry) or **git** instead of path, that dependency is no longer a "path dependency" and the script does not enforce that it is in the allowlist. You can end up with Hub depending on non-framework crates by version while the allowlist is only enforced for path deps.

**Why it happens:**  
The check is defined in terms of "path deps that are workspace members" vs allowlist. The design assumes Hub and binaries depend on the framework via workspace path. Switching to "framework as published dependency" changes the dependency source, not the allowlist semantics—but the implementation only inspects path deps.

**How to avoid:**  
- **Option A (same repo, framework publishable):** Keep Hub and binaries depending on framework crates via **path + version**. Boundary check stays as-is (path deps must be in allowlist).  
- **Option B (separate repo or Hub consuming by version):** Extend the boundary check so that **all** dependencies (path and registry/git) of Hub and binaries are checked: only allowlisted crate names are allowed, regardless of source. Update allowlist semantics and docs (e.g. `docs/framework-surface.md`) to "allowed crate names" not "path dependencies only."

**Warning signs:**  
- Hub or binaries `Cargo.toml` gains `neurohid-core = "0.2"` (or git) without CI or allowlist logic updated.  
- Discussion of "Hub consumes framework as dependency" without deciding same-repo (path+version) vs separate-repo (version/git) and updating the check accordingly.

**Phase to address:**  
Phase that defines "Hub consumes framework as dependency" and/or "clear API boundary and release story." Align boundary enforcement with the chosen consumption model (path+version vs version-only) and update CI and docs in that phase.

---

### Pitfall 4: Version skew between path and published

**What goes wrong:**  
Local dev uses path; after publish, external (or CI) consumers use version. If the version in `Cargo.toml` (or `workspace.package`) is out of sync with what was actually published, or if some framework crates are bumped and others are not, you get "version not found," broken resolution, or subtle ABI/behavior mismatch.

**Why it happens:**  
Multiple places can hold version numbers (root `[workspace.package]`, each crate’s `version`, dependency `version = "x.y.z"`). Manual or partial bumps, or publishing a subset of crates, create skew.

**How to avoid:**  
- Single source of truth for framework crate versions (e.g. `version.workspace = true` and one `[workspace.package]` version, or a release script that sets versions from one input).  
- For path+version deps, use the same version source (e.g. workspace) so that "bump once" updates both the crate and its dependents’ requirement.  
- In release automation: bump all publishable framework crates that changed (or the whole framework surface) together, then publish in topological order.

**Warning signs:**  
- Different version numbers in root vs in a crate’s `Cargo.toml`.  
- Dependency `version = "0.2.0"` while the depended-on crate is already `0.3.0`.  
- No automation or checklist for "bump and publish" that keeps path and published in sync.

**Phase to address:**  
Versioning and release phase. Define versioning policy (single version vs independent semver) and implement it in manifests and release steps.

---

### Pitfall 5: Cyclic dev-dependencies break publish

**What goes wrong:**  
Locally, `cargo build` and `cargo test` succeed, but `cargo publish --dry-run` fails or publish fails because a cycle appears when both dependencies and dev-dependencies must be resolvable from the registry (e.g. `foo` depends on `bar`, `bar`’s dev-dependencies depend on `foo`).

**Why it happens:**  
Cargo allows cycles that involve only dev-dependencies for local builds (build order can still be acyclic). For publish, all deps must be on the registry, so the cycle becomes a real constraint.

**How to avoid:**  
- Avoid dev-dependency cycles between workspace crates that are publishable or that publishable crates depend on.  
- If present: remove the cycle (e.g. move shared test helpers to a small crate, or stub tests for publish), or use a publish-time workaround (e.g. strip dev-dependencies before `cargo publish` and document it)—prefer removing the cycle.

**Warning signs:**  
- `cargo publish -p <crate> --dry-run` fails with resolution or cycle errors after local tests pass.  
- `cargo metadata` or dependency graph tools show a cycle involving dev-dependencies between framework crates.

**Phase to address:**  
Phase that makes framework crates publishable. Audit dev-dependencies of all publishable (and their in-repo dependents) for cycles; fix before first publish.

---

### Pitfall 6: Forgetting publish metadata on newly publishable crates

**What goes wrong:**  
`cargo publish` rejects a crate because required metadata is missing: e.g. `license` or `license-file`, `description`, `repository` (and often `readme`, `homepage`, `keywords`). crates.io also requires that dependencies specify a version when publishing.

**Why it happens:**  
Crates that were `publish = false` never needed crates.io metadata. Flipping to publishable without adding metadata causes immediate publish failure.

**How to avoid:**  
Before marking a crate publishable: (1) set `publish = true` only after adding required fields; (2) use `cargo publish -p <crate> --dry-run` and fix every warning/error; (3) Prefer inheriting shared fields from `[workspace.package]` (e.g. `license.workspace = true`, `repository.workspace = true`) so one place stays correct.

**Warning signs:**  
- New or newly publishable crate has no `description` or `license` in its `Cargo.toml`.  
- Dry-run not run (or not in CI) for every publishable crate before first release.

**Phase to address:**  
First phase that makes framework crates publishable. Checklist: required metadata + dry-run for each crate; add a CI job that runs `cargo publish --dry-run` for all publishable packages.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|------------------|
| Publish only leaf crates (e.g. `neurohid`, `neurohid-sdk`) and keep all framework crates path-only | No need to add version to many path deps or fix publish order | Downstream cannot depend on framework by version; no clean "framework" package for Python bindings or other repos | Only if framework is never consumed by version |
| Single version for all framework crates (e.g. 0.2.0 everywhere) | Simple bumps; one number to manage | Any breaking change in one crate forces a major/minor bump for the whole surface; less flexible semver | Acceptable and common for a cohesive framework; document policy |
| Temporarily stripping dev-dependencies before publish to break cycles | Unblocks publish quickly | Easy to forget; different behavior from normal build; maintenance burden | Never preferred; remove cycle instead |
| Keeping Hub on path deps only (no version) while framework is publishable | Boundary check unchanged; no CI changes | Hub never tests "consuming published framework"; possible skew between path and published behavior | Acceptable near term; add optional CI that builds Hub against published versions later |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| **Hub ↔ framework** | Switching Hub to `neurohid-core = "0.2"` (registry) without updating boundary check | Either keep Hub on `path + version` and keep current check, or add check for registry deps so only allowlisted names are allowed |
| **CI (framework-boundary)** | Assuming "path deps" and "allowed deps" are the same forever | When adding publish path, decide: same-repo path+version (check stays) or version/git (extend check to all dependency sources) |
| **CI (publish)** | Running `cargo publish` for multiple crates in arbitrary or parallel order | Publish in topological order; add delay/retry after each publish for registry visibility |
| **Versioning** | Bumping only one or two crates when many depend on them | Bump all affected framework crates in one change; publish in dependency order |
| **neurohid-sdk / neurohid (binaries)** | They currently have path-only deps; adding framework publish without fixing them | Add `version` to every workspace path dep in neurohid and neurohid-sdk before (or as part of) framework publish; run dry-run for both |

---

## Moderate Pitfalls

### Forgetting `publish = false` on Hub when framework is publishable

**What goes wrong:**  
If Hub is accidentally set to `publish = true`, someone might publish it to crates.io. Hub is an application that depends on the framework; it usually should not be a published library.

**Prevention:**  
Keep `publish = false` in `neurohid-hub/Cargo.toml` unless you explicitly decide to publish a "hub library." Document in crate-boundaries or framework-surface that Hub is an app, not a published crate.

---

### workspace.package and workspace.dependencies

**What goes wrong:**  
If some crates use `version.workspace = true` and others override with a literal version, or if `[workspace.dependencies]` entries for workspace members omit `version`, you get inconsistent or unpublishable manifests.

**Prevention:**  
Use `version.workspace = true` for all publishable framework crates and one `[workspace.package]` version (or a small set). For workspace member deps, use `path` + `version` (or a workspace dependency that includes version). Do not mix literal versions for framework crates with workspace inheritance without a clear policy.

---

## "Looks Done But Isn't" Checklist

- [ ] **Publishable framework:** Every workspace path dependency in a publishable crate has a `version` key — verify with `cargo publish -p <crate> --dry-run` for each.
- [ ] **Boundary enforcement:** If Hub can depend on framework by version/git, CI checks **all** deps (path + registry) against allowlist — verify script and docs.
- [ ] **Release order:** Publish workflow or script uses topological order and delay/retry — verify with a test run or doc.
- [ ] **Version single source:** All framework versions come from one place (e.g. workspace.package) and path deps reference it — verify grep for literal versions in framework crates.
- [ ] **Dev-dependency cycles:** No cycle among framework crates involving dev-deps — verify with `cargo metadata` or dependency-graph and dry-run.
- [ ] **Metadata:** Every publishable crate has license, description, repository (and readme if applicable) — verify dry-run and CI.

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Path-only deps at publish | LOW | Add `version` to each path dep (and optionally `version.workspace = true`), re-run dry-run. |
| Wrong publish order / registry delay | LOW | Re-run publish in correct order; add retry/sleep in automation. |
| Boundary check no longer applies (Hub on version) | MEDIUM | Extend check to all deps by name; update allowlist semantics and docs; add tests. |
| Version skew | MEDIUM | Bump and re-publish affected crates in order; document version policy to avoid repeat. |
| Dev-dependency cycle | MEDIUM | Remove cycle (extract test helper crate or drop cyclic dev-dep); optionally yank broken version if already published. |
| Missing metadata | LOW | Add fields, re-run dry-run, re-publish. |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Path-only deps when publishing | Phase: Make framework publishable / manifest & versioning | `cargo publish -p <each> --dry-run` passes; CI runs dry-run for all publishable crates |
| Publish order and delay | Phase: Release story / publish automation | Publish job or script documents and uses topological order + retry; test run succeeds |
| CI boundary only path deps | Phase: Hub consumes framework / API boundary & release | If Hub uses version: boundary script checks all deps; if path+version: script unchanged and documented |
| Version skew path vs published | Phase: Versioning & release | Single version source; checklist or automation bumps consistently |
| Cyclic dev-dependencies | Phase: Make framework publishable | Dry-run passes for all; dependency graph has no cycle among publishable + deps |
| Missing publish metadata | Phase: Make framework publishable | Required fields set; dry-run and CI enforce |

---

## Sources

- [Publishing on crates.io - The Cargo Book](https://doc.rust-lang.org/cargo/reference/publishing.html) — metadata and packaging.
- [RFC 2906: workspace dependencies and metadata](https://rust-lang.github.io/rfcs/2906-cargo-workspace-deduplicate.html) — path + version for workspace members.
- [Gotchas to Publish Rust Crates in a Workspace](https://blog.iany.me/2020/10/gotchas-to-publish-rust-crates-in-a-workspace/) — version on path deps, publish order, 10s delay, cyclic dev-deps, slow publish.
- [Lindera issue #358](https://github.com/lindera/lindera/issues/358) — workspace.dependencies and versions when publishing.
- [Cargo issue #1169](https://github.com/rust-lang/cargo/issues/1169) — no `cargo publish --all`; manual order.
- [Cargo issue #4242](https://github.com/rust-lang/cargo/issues/4242) — cyclic dev-dependencies and publish.
- Project: `.github/framework-allowlist.toml`, `.github/scripts/check-framework-boundary.ps1`, `docs/framework-surface.md`, `docs/crate-boundaries.md`, `crates/*/Cargo.toml` (path deps, publish flags).

---
*Pitfalls research for: adding framework repo split or publishable package to NeuroHID monorepo; integration (Hub, CI, versioning, path vs published).*
*Researched: 2026-02-22*
