# Stack Research: v1.1 Additions (Testing, BrainFlow, Framework Boundary)

**Domain:** NeuroHID v1.1 — testing depth, native BrainFlow integration, framework-vs-Hub structural separation  
**Researched:** 2026-02-21  
**Confidence:** HIGH (testing, framework); MEDIUM (BrainFlow Rust integration path)

## Scope

This document covers **only** stack additions or changes required for the three v1.1 feature areas. Existing stack (Rust 2024/1.85+, Python 3.12+, uv, neurohid-* crates, eframe/egui, tract-onnx, IPC, etc.) is unchanged and not re-researched.

---

## 1. Thorough Testing (Rust + Python, flakiness, integration/E2E)

### Recommended Additions

#### Rust

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| **cargo-nextest** | 0.9.x (latest stable) | Test runner | Faster than `cargo test` (parallel per-test), built-in retries for flaky tests, JUnit XML for CI, timeout controls; integrates with existing `cargo llvm-cov` so CI can run nextest then coverage. |
| **cargo-llvm-cov** | (already in CI) | Code coverage | Keep current setup; officially recommended, works with nextest (`cargo llvm-cov --no-run` + `cargo nextest run` or use nextest’s coverage support where available). No new add. |
| **tokio-test** | 0.4 (existing) | Async test utilities | Keep; no change. |
| **approx** | 0.5.1 (workspace) | Float assertions | Keep; no change. |

- **Integration / E2E:** No new framework. Continue current pattern: integration tests as `--test` crates (e.g. `neurohid-core --test extension_outlet_e2e`), and targeted binary tests (e.g. `neurohid --bin neurohid-service`). Add more such tests where valuable; no extra stack.
- **Flakiness:** Use nextest’s `retries` in config (e.g. `.config/nextest.toml` or `nextest.toml` in repo) for known-flaky tests; prefer fixing root cause and use retries sparingly.

#### Python

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| **pytest** | ≥9.0.2 (existing) | Test runner | Keep. |
| **pytest-cov** | ≥7.0.0 (existing) | Coverage | Keep; already in CI with Codecov. |
| **pytest-asyncio** | ≥1.3.0 (existing) | Async tests | Keep. |
| **pytest-rerunfailures** | ≥14.0 | Flaky retries | Rerun failing tests N times (CLI or `@pytest.mark.flaky`); use sparingly; avoid with pytest-xdist if mixing parallel + reruns. |

- **Flakiness:** Prefer fixing order/races; add `pytest-rerunfailures` only for identified flaky tests. Document in `python/pyproject.toml` and optionally in `conftest.py` (e.g. `--reruns 2` for specific marks).

### What NOT to Add (Testing)

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| A separate E2E “framework” (e.g. Playwright for Hub) for v1.1 | Scope and maintenance cost; Hub already has egui_kittest. | More unit/integration tests and existing binary/integration test pattern. |
| cargo-tarpaulin | We already use cargo-llvm-cov in CI; Tarpaulin weaker on non-Linux. | Keep cargo-llvm-cov. |
| pytest-xdist + rerunfailures together by default | Rerun behavior can be inconsistent when parallel. | Add rerunfailures only where needed; avoid parallel for flaky suites if both are used. |

### Integration with Existing Stack

- **Rust:** Add `nextest` as a dev/CI tool (install via `cargo install cargo-nextest` or CI action). Optionally add `nextest.toml` at repo root with `retries = 2` for selected tests and `run.tests.timeout`; CI can run `cargo nextest run --workspace` instead of `cargo test --workspace`, and keep existing `cargo llvm-cov` job (nextest is compatible with llvm-cov workflows).
- **Python:** Add `pytest-rerunfailures` to `[project.optional-dependencies]` or dev deps in `python/pyproject.toml`; no change to `uv` or test entrypoints (`uv run --project python pytest ...`).

---

## 2. Native BrainFlow Integration (docs/examples/Hub UX, then board config/streaming)

### Current State

- **neurohid-device** has a BrainFlow **simulation** adapter (feature `brainflow`): wraps Mock with BrainFlow-like board metadata; **no** real BrainFlow SDK linked.
- **neurohid-types** has `BrainFlowConfig` (e.g. `board_id`, `serial_port`).
- Hub Settings already expose a BrainFlow backend option and config; backend is mock-based today.

### Recommended Stack for Native BrainFlow

#### Rust (real SDK integration)

| Technology | Version / Source | Purpose | Why |
|------------|------------------|---------|-----|
| **BrainFlow core (C/C++)** | Build from source (BrainFlow repo `tools/build.py`) | Native libs | BrainFlow has no prebuilt Rust crate on crates.io; Rust binding is in-tree and requires built C/C++ core and dynamic libs. |
| **BrainFlow Rust binding** | In-tree: `brainflow-dev/brainflow` → `rust_package/brainflow` | BoardShim, API | Official binding; build with `cargo build --features generate_binding` after core is built; crate name `brainflow`, not on crates.io. |

- **Dependency option A (recommended for v1.1):** Add BrainFlow as a **git dependency** pointing at `brainflow-dev/brainflow` (e.g. `brainflow = { git = "https://github.com/brainflow-dev/brainflow", rev = "<tag-or-commit>" }`) in `neurohid-device` under a new feature (e.g. `brainflow-native`) so default build stays mock-only. Build requires: (1) user/CI builds BrainFlow core and sets `BRAINFLOW_DIR` or installs libs to a standard path; (2) neurohid-device’s build script finds libs and links. This implies documenting build order and platform steps (Windows/Linux/macOS) in `docs/` and optionally a small CI job that builds BrainFlow then builds `neurohid-device` with `brainflow-native`.
- **Dependency option B:** Vendor `rust_package/brainflow` into repo (submodule or copy) and depend via `path = "..."`; same “build core first” requirement, more control at cost of upkeep.
- **Do not** add a third-party “brainflow” crate from crates.io for the official API; the only official Rust support is in the BrainFlow repo.

#### Python (already aligned)

- **brainflow** on PyPI (e.g. ≥5.20 per existing research) for docs, examples, and any Python-side Hub/scripting. No stack change; ensure docs/examples reference `uv run --project python` and optional `brainflow` dependency if examples use it.

### Documentation and Examples (stack-adjacent)

- **Docs:** Add/update `docs/brainflow.md` (or equivalent): build order (BrainFlow core → Rust binding), env vars, platform notes, feature flags (`brainflow` vs `brainflow-native`), and how Hub discovery/connection UX maps to `BrainFlowConfig` / `BrainFlowProvider`.
- **Examples:** Add runnable examples (Rust and/or Python) that use the native BrainFlow API (BoardShim, get_board_data) for discovery and streaming; these live in repo and are referenced from docs.
- **Hub UX:** No new GUI framework; extend existing egui Settings/Devices screens and config types. No stack addition.

### What NOT to Add (BrainFlow)

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| A separate “brainflow-sys” crate from crates.io | No official crate; in-tree binding uses bindgen. | Git/path dep on BrainFlow’s `rust_package/brainflow`. |
| Bundling BrainFlow C/C++ in our repo | Licensing and build complexity. | Document “build BrainFlow first”; optional CI that builds BrainFlow then neurohid. |
| Replacing Mock BrainFlow adapter with native-only | CI and no-hardware dev need the mock. | Keep mock adapter; add native path behind `brainflow-native` (or similar) feature. |

### Integration with Existing Crates

- **neurohid-device:** Add optional dep `brainflow = { git = "..." }` under feature `brainflow-native`; in `brainflow.rs` (or a new `brainflow_native.rs`), implement `DeviceProvider`/`Device` using BoardShim when `brainflow-native` is enabled, keeping current mock adapter when only `brainflow` is enabled or for tests.
- **neurohid-types:** Keep `BrainFlowConfig`; no change.
- **neurohid-core / neurohid-hub:** No new direct BrainFlow deps; they use device abstraction only.
- **Python:** Use `brainflow` from PyPI in examples/docs where needed; optional dev/example dep in `python/pyproject.toml`.

---

## 3. Framework vs Hub Structural Separation

### Recommendation: Layout and Documentation Only

No new frameworks or crates.io dependencies. The existing crate graph already supports a clear boundary:

- **Framework** = reusable surface: `neurohid-types`, `neurohid-device`, `neurohid-signal`, `neurohid-platform`, `neurohid-ipc`, `neurohid-storage`, `neurohid-calibration`, `neurohid-core`, `neurohid-sdk`.
- **Application** = Hub and binaries: `neurohid-hub`, `neurohid` (binaries: neurohid, neurohid-service, neurohid-validate).

Dependency direction is already correct (Hub depends on core/sdk, not directly on device/signal per `docs/crate-boundaries.md`).

### Optional Tooling (No New Deps)

| Approach | Purpose | Notes |
|----------|---------|--------|
| **Workspace group** | Clarify “framework” vs “app” in one place | In root `Cargo.toml`, `[workspace]` has no formal groups; could add a comment block or a `[target]`/metadata listing framework crates. No Cargo feature for this. |
| **Docs** | Single source of truth | Add or update `docs/crate-boundaries.md` (or `docs/framework-surface.md`) with an explicit “Framework surface” section: list framework crates, state that Hub is one application on top, and that a future full split (e.g. framework repo) will preserve this boundary. |
| **Optional facade crate** | Single “framework” entrypoint | Could introduce `neurohid-framework` that re-exports only the public API of the framework crates (similar to SDK but scoped to “what the framework offers”). This is a structural choice, not a new external dependency; defer unless product needs a single dependency name. |

### What NOT to Add

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| A new “framework” framework or meta-crate from crates.io | Goal is boundary clarity, not a new runtime. | Document boundary; optionally one facade crate. |
| Moving crates to a different repo in v1.1 | Planned for a later milestone. | In-repo layout and docs only. |

### Integration with Existing Stack

- No changes to `Cargo.toml` dependencies for framework boundary only.
- CI and build remain `cargo build --workspace` / `cargo test --workspace`; no new tools.
- `neurohid-sdk` continues to be the public re-export surface for embedders; framework boundary doc can state that SDK is the recommended external dependency and that Hub is one consumer of the same framework.

---

## Summary Table: v1.1 Stack Additions

| Area | Add | Version / Source | Where |
|------|-----|------------------|--------|
| Testing (Rust) | cargo-nextest | 0.9.x | Dev/CI tool; optional `nextest.toml` |
| Testing (Python) | pytest-rerunfailures | ≥14.0 | Optional/dev dep in `python/pyproject.toml` |
| BrainFlow (Rust) | BrainFlow Rust binding | git: brainflow-dev/brainflow, rust_package/brainflow | neurohid-device, feature `brainflow-native` |
| BrainFlow (build) | BrainFlow C/C++ core | Build from source (tools/build.py) | Docs + optional CI |
| Framework boundary | (none) | — | Docs + optional workspace metadata or facade crate |

---

## Installation / Quick Reference

```bash
# Rust: nextest (dev/CI)
cargo install cargo-nextest

# Python: flaky retries (optional)
uv add --project python --optional dev pytest-rerunfailures

# BrainFlow: not installed via Cargo; build BrainFlow core then use git dep (see docs).
```

---

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| cargo-nextest | Keep `cargo test` only | If CI is already fast enough and flakiness is minimal. |
| cargo-llvm-cov (keep) | cargo-tarpaulin | Tarpaulin if Linux-only and ptrace is preferred; we already use llvm-cov and need cross-platform. |
| pytest-rerunfailures | flaky (PyPI) | Both work; rerunfailures is more common and CLI-friendly. |
| BrainFlow git/path dep | Vendor full BrainFlow repo | Vendor if we need to patch Rust binding or pin an unreleased fix. |
| Document framework boundary | New neurohid-framework crate | Add facade crate only if we want a single “framework” dependency name before a future repo split. |

---

## Version Compatibility

| Item | Compatible With | Notes |
|------|-----------------|--------|
| cargo-nextest 0.9.x | Rust 1.85+ | No conflict with workspace rust-version. |
| cargo-llvm-cov (existing) | cargo nextest | Use `cargo llvm-cov` with test run from nextest or keep current CI flow. |
| pytest-rerunfailures ≥14.0 | pytest ≥9 | Matches current pytest. |
| BrainFlow rust_package | BrainFlow core (built) | Must build core first; Rust crate has older deps (e.g. ndarray 0.15) — may need to wrap or use only in brainflow-native feature to avoid pulling old deps into main graph. |

**Note on BrainFlow Rust deps:** The in-tree `brainflow` crate uses older `ndarray` and other deps. Prefer depending on it only under `brainflow-native` and not re-exporting its types in the main API surface; wrap BoardShim in neurohid-device’s existing types to keep the rest of the workspace on current ndarray/serde.

---

## Sources

- cargo-nextest: [nexte.st](https://nexte.st), [docs](https://nexte.st/docs/running), [changelog](https://nexte.st/changelog) — HIGH.
- cargo-llvm-cov: [crates.io](https://crates.io/crates/cargo-llvm-cov), [taiki-e/install-action](https://github.com/taiki-e/install-action) (existing in CI) — HIGH.
- Rust coverage: Rust Project Primer, rustc instrument-coverage; cargo-tarpaulin vs llvm-cov — MEDIUM.
- pytest-rerunfailures: [pytest-rerunfailures docs](https://pytest-rerunfailures.readthedocs.io/stable) — HIGH.
- BrainFlow: [BuildBrainFlow](https://brainflow.readthedocs.io/en/stable/BuildBrainFlow.html) (Rust: build from source), [brainflow-dev/brainflow rust_package](https://github.com/brainflow-dev/brainflow/tree/master/rust_package), [Data Format](https://brainflow.readthedocs.io/en/stable/DataFormatDesc.html) — HIGH for build path; MEDIUM for Rust crate version/dep alignment.
- Framework boundary: `docs/crate-boundaries.md`, `.planning/codebase/ARCHITECTURE.md` — HIGH.

---
*Stack research for: NeuroHID v1.1 (testing, BrainFlow, framework boundary)*  
*Researched: 2026-02-21*
