# Stack Research: v1.2 Framework Publishable & Release (Prep for Python Bindings)

**Domain:** Framework repo split or publishable package; clear release story; prep for Python bindings (no bindings in this milestone).  
**Researched:** 2026-02-22  
**Confidence:** HIGH (Cargo/maturin official docs); MEDIUM (release tooling choice).

## Scope

This document covers **only** stack additions or changes needed for:

- Splitting the framework into a separate repo **or** making it a publishable package (crates.io).
- Hub (and other apps) consuming the framework as a dependency with a clear API boundary and release story.
- Preparing for a future milestone that adds Python bindings (PyO3/maturin, C API, or similar)—**this milestone does not implement bindings**.

Existing stack (Rust 2024/1.85+, workspace, neurohid-* crates, framework surface and allowlist, Python/uv, neurohid-ml) is unchanged unless stated below.

---

## Recommended Stack

### Core: Publishing the Framework

| Technology | Version / practice | Purpose | Why |
|------------|--------------------|---------|-----|
| **Cargo path + version** | (manifest syntax) | Publishable workspace crates | Dependencies like `neurohid-types = { path = "../neurohid-types", version = "0.1.0" }` use the path locally and the **version** when the crate is published; crates.io does not allow path-only deps. Required for any framework crate that is published while others in the workspace depend on it. |
| **workspace.package** | (existing) | Shared version, edition, license, repository | Already in root `Cargo.toml`; use for all publishable crates so version/edition/rust-version stay consistent. Override per crate only when needed (e.g. different version for a facade). |
| **publish** field | `true` / `false` | Which crates are allowed on crates.io | Framework crates that will be published: set `publish = true` and add `description`, `repository`, and optionally `readme`, `keywords`, `categories`. Keep `publish = false` for Hub, binaries-only crates you do not publish, and internal tools. |
| **Semantic Versioning** | 2.0 | Version bumps | Follow Cargo semver rules for compatibility; 0.x.y → minor = breaking, patch = compatible. |

**Publishable set (recommended):** All framework-surface crates that embedders or the SDK re-exports depend on: `neurohid-types`, `neurohid-device`, `neurohid-signal`, `neurohid-platform`, `neurohid-ipc`, `neurohid-storage`, `neurohid-calibration`, `neurohid-core`. Plus the existing `neurohid-sdk` (facade) and optionally the `neurohid` binary crate. Do **not** publish `neurohid-hub` or `neurohid-outlet-example` as library dependencies; they can remain `publish = false` or, for `neurohid`, be published as a binary-only crate.

### Release Story

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| **release-plz** or **cargo-release** | release-plz 0.9+ / cargo-release 0.25+ | Version bump, changelog, publish order, git tags | Cargo Book recommends automation; both support workspaces. release-plz: PR-based workflow, conventional commits, optional cargo-semver-checks. cargo-release: simpler CLI, `--workspace` and `--package`/`--exclude`. Choose one for CI or local release. |
| **Keep a Changelog** | (existing) | Human-readable release notes | Repo already uses this format in `CHANGELOG.md`; keep it and tie release tool to it (release-plz uses git-cliff / changelog section; cargo-release can update a single changelog). |
| **Git tags** | e.g. `v0.1.0` or `neurohid-core-v0.1.0` | Traceability | One tag per published version (or per workspace release); Cargo Book recommends tags for each publish. |
| **cargo publish --workspace** (optional) | Cargo 1.90+ (Rust 1.90, Sept 2025) | Publish all workspace crates in dependency order in one go | Native Cargo feature; verifies and uploads in correct order. If toolchain is 1.90+, use this to avoid manual order or third-party scripts. If staying on 1.85, use release-plz or cargo-release to manage order. |

### Python Bindings Prep (No New Code This Milestone)

| Technology | When to add | Purpose | Why |
|------------|-------------|---------|-----|
| **maturin** | When implementing bindings | Build and publish Python wheels from Rust | Standard for PyO3; supports mixed Rust/Python repo; can use `python-source = "python"` to match existing `python/` layout. |
| **PyO3** | When implementing bindings | Rust ↔ Python FFI | Default choice for native Python extensions; maturin auto-detects. Use a dedicated crate (e.g. `neurohid-python` or under `python/`) with `crate-type = ["cdylib"]` and pyo3 as dependency. |
| **cbindgen** | Only if choosing C API | Generate C headers from Rust | For CFFI or ctypes; maturin supports CFFI with cbindgen. Do not add unless you commit to a C API surface. |

**This milestone:** No new crates, no maturin/PyO3 in the workspace. Only ensure: (1) framework is publishable and versioned, (2) public API of the crate(s) that will be bound is stable enough to document, (3) repo layout can later host a binding crate (e.g. under `crates/` or a dedicated Python package with its own `Cargo.toml` + `pyproject.toml`).

### Development / CI Tools

| Tool | Purpose | Notes |
|------|---------|--------|
| **cargo publish --dry-run** | Verify package before upload | Run for each publishable crate (or with `--workspace` on 1.90+) before first real publish. |
| **cargo metadata --format-version=1** | Inspect dependency graph | Existing CI (e.g. allowlist check) already uses this; no change. |
| **release-plz** or **cargo-release** | Automate version + changelog + publish | Add to CI or run locally; configure `release-plz.toml` or equivalent so only framework crates (and SDK/binary) are published. |

---

## Installation / Setup

```bash
# Release automation (pick one)
cargo install release-plz
# or
cargo install cargo-release

# When adding Python bindings later
uv tool add maturin
# or: pip install maturin
```

No change to existing `cargo build`/`cargo test` or `uv` usage.

---

## Workspace and Dependency Layout

- **Same-repo publishable (recommended):** Keep the current monorepo. For each publishable framework crate, change path-only deps to `{ path = "...", version = "X.Y.Z" }` (version from workspace or crate). Publish order: types → components → core → sdk (and binaries if desired). Hub continues to depend on framework crates via **path** in repo; when consuming from another repo or as a crates.io user, depend on **version**.
- **Separate repo:** If the framework is split to another repo, Hub’s repo would depend on the framework via `git` or `version` (crates.io). Then framework crates are published from the framework repo; no path deps across repos. More operational overhead (two repos, release coordination).
- **Hub consuming framework:** Today Hub uses path deps within the allowlist. After framework is published, Hub can either keep path deps (for in-repo dev) or switch to version deps; using **path + version** in the same dep line keeps both worlds (local path when in workspace, version when Hub is ever published or used from another workspace).

---

## Alternatives Considered

| Recommended | Alternative | When to use alternative |
|-------------|-------------|---------------------------|
| Publish framework crates from same repo | Split framework into a separate git repo | If you need strict org boundaries or independent release cadence for framework vs Hub. |
| release-plz | cargo-release | cargo-release is simpler and more CLI-focused; use if you prefer manual or script-driven releases. |
| release-plz | Manual version bump + cargo publish | Acceptable for very small teams; error-prone for many crates. |
| PyO3 + maturin (later) | C API + cbindgen + CFFI/ctypes | Use C API if you need non-Python consumers (e.g. other languages) or PyPy/abi3 constraints that PyO3 can also address; maturin supports both. |

---

## What NOT to Add (This Milestone)

| Avoid | Why | Use instead |
|-------|-----|-------------|
| PyO3 or maturin in the workspace now | Milestone is prep only; no bindings implemented. | Document PyO3 + maturin as the intended stack and keep API surface stable. |
| A new “neurohid-python” or binding crate in v1.2 | Same as above. | Add in the milestone that implements bindings. |
| cbindgen or C API layer now | Adds surface to maintain without a consumer. | Add when you commit to C FFI. |
| Removing or relaxing the Hub allowlist | Boundary is already validated and documented. | Keep `.github/framework-allowlist.toml` and CI; Hub still depends only on allowlist crates. |
| Publishing neurohid-hub as a library | Hub is an application, not a dependency. | Keep `publish = false` for neurohid-hub. |
| Upper-bound version pins (e.g. `"=0.1.0"`) for workspace crates | Prevents compatible updates. | Use caret/default requirement (e.g. `"0.1.0"`) unless you have a specific reason. |

---

## Version Compatibility

| Package / tool | Compatible with | Notes |
|----------------|-----------------|--------|
| Cargo workspace publish | Rust 1.90+ (Cargo 1.90) | Stable Sept 2025; if toolchain is 1.85, use release-plz or cargo-release for order. |
| release-plz | Cargo 1.70+ | Works with current workspace. |
| cargo-release | Cargo 1.70+ | Same. |
| maturin (future) | PyO3 0.23+; Python 3.8+ | When adding bindings; maturin 1.12+ current. |
| path + version in same dep | All stable Cargo | Required for publishable workspace crates. |

---

## Integration with Existing Repo

- **Rust workspace:** Add `version` to every **path** dependency that points at a crate you will publish. Use `version.workspace = true` or an explicit version so the published manifest has a registry version requirement.
- **neurohid-sdk:** Today it has `publish = true` and path-only optional deps; switch those to `path = "...", version = "..."` for each published framework crate so that when SDK is published it resolves from crates.io.
- **neurohid (binaries):** Same idea if you publish it: path + version for hub, core, etc.
- **CI:** Keep framework-allowlist check; add a job or step that runs `cargo publish --dry-run` for each publishable package (or `cargo publish --workspace --dry-run` on 1.90+) on release branches or tags.
- **Changelog:** Keep `CHANGELOG.md` at repo root; configure release-plz (or cargo-release) to update it from conventional commits or manually.
- **Python side:** No change to `python/` or neurohid-ml this milestone; when bindings are added, a new crate + optional `pyproject.toml` for maturin can live alongside existing Python code.

---

## Sources

- [Publishing on crates.io - The Cargo Book](https://doc.rust-lang.org/cargo/reference/publishing.html) — publish steps, git tags, changelog, release-plz/cargo-release/cargo-smart-release.
- [Specifying Dependencies - Path and version](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html) — path + version for publishable workspace crates; “Multiple locations”.
- [Tweag: Publish all your crates everywhere (Cargo 1.83/1.90)](https://tweag.io/blog/2025-07-10-cargo-package-workspace/) — workspace publish behavior and ordering.
- [Maturin User Guide - Project Layout](https://www.maturin.rs/project_layout.html) — mixed Rust/Python, `python-source`, pure Rust with pyproject.toml.
- [Maturin User Guide - Bindings](https://www.maturin.rs/bindings.html) — pyo3, cffi, cbindgen.
- [Release-plz configuration](https://release-plz.dev/docs/config) — workspace and package options.
- [Announcing Rust 1.90.0](https://blog.rust-lang.org/2025/09/18/Rust-1.90.0/) — Cargo workspace publishing stable.

---
*Stack research for: v1.2 framework consumable (prep for Python bindings). Researched: 2026-02-22.*
