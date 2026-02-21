# Development Guide

This guide is the canonical reference for local development setup, build/test workflows, and
automation-oriented quality gates.

## Prerequisites

- Rust `1.85+`
- Python `3.12+`
- `uv` for Python environment and command execution
- PowerShell (`pwsh`) for repository automation scripts under `.github/scripts/`

If you are working with LSL-backed device paths, install platform LSL tooling and note that
workspace builds pin `lsl-sys` via `[patch.crates-io]` to a shared git source for reproducible
cross-app behavior.

## Local Setup

From repository root:

```bash
cargo build --workspace
uv sync --directory python
```

## Build Commands

```bash
# Full workspace (debug)
cargo build --workspace

# Full workspace (release)
cargo build --release

# Build a single crate
cargo build -p neurohid-core

# Example no-LSL build for device crate
cargo build -p neurohid-device --no-default-features
```

## Common Local Run Commands

```bash
# GUI hub
cargo run -p neurohid --bin neurohid

# Headless service
cargo run -p neurohid --bin neurohid-service

# Validation harness
cargo run -p neurohid --bin neurohid-validate -- --help

# Python ML bridge
uv run --directory python neurohid-ml bridge
```

## Validation and Testing

### Rust

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

### Python

```bash
uv run --project python pytest python/tests -q
uv run --project python ruff check python/src python/tests
uv run --project python black --check python/src python/tests
uv run --project python mypy python/src
```

## Automation Harness Commands

Run canonical local quality gates (same script family used by CI):

```bash
# Focused Rust + docs/protocol/unsafe policy checks
pwsh -File ./.github/scripts/run-agent-ready-tasks.ps1 -RustScope focused -WithDocs -WithProtocol -WithUnsafe

# Python-only quality gates
pwsh -File ./.github/scripts/run-agent-ready-tasks.ps1 -SkipRust -WithPython
```

Impact-aware routing logic is implemented in `.github/scripts/classify-impact.ps1`.

## CI Runner Policy

- Private phase uses self-hosted GitHub Actions runners with dedicated labels:
  - Linux: `self-hosted`, `linux`, `neurohid-ci`
  - Windows: `self-hosted`, `windows`, `neurohid-ci`
  - macOS: `self-hosted`, `macos`, `neurohid-ci`
- **Fallback:** The Test job also runs on GitHub-hosted `ubuntu-22.04` so that Linux tests can
  complete when self-hosted Linux runners are unavailable. Self-hosted remains preferred for
  performance and LSL/device tooling.
- `ci.yml` controls optional macOS execution with repository variable `ENABLE_MACOS`.
- If macOS support is dropped temporarily, set `ENABLE_MACOS=false` in repo variables and align
  required checks in branch protection.
- At release/public transition, change workflow runner mappings to GitHub-hosted labels
  (`ubuntu-latest`, `windows-latest`, `macos-latest`).

### Self-hosted runner setup

To register self-hosted runners for NeuroHID CI:

1. **Repository:** GitHub → Settings → Actions → Runners → New self-hosted runner.
2. **OS:** Follow the displayed commands for the runner OS (Linux, Windows, or macOS).
3. **Labels:** Use the labels above so workflows can select them:
   - Linux: `self-hosted`, `linux`, `neurohid-ci`
   - Windows: `self-hosted`, `windows`, `neurohid-ci`
   - macOS: `self-hosted`, `macos`, `neurohid-ci`
4. **Linux:** Install build deps (e.g. `liblsl-dev`) and Rust/Python/uv so that `cargo build
   --workspace`, `cargo test --workspace`, and `uv sync --directory python` succeed.
5. **Windows:** Install Visual Studio Build Tools (or MSVC) and LSL if needed; ensure PowerShell
   and Git are available.
6. **macOS:** Install Xcode Command Line Tools and LSL; ensure `cargo` and `uv` are on the PATH.

After registration, jobs in `.github/workflows/ci.yml` that use the corresponding `runs-on` labels
will run on these runners. See [GitHub: Adding self-hosted runners](https://docs.github.com/en/actions/hosting-your-own-runners/adding-self-hosted-runners).

## Coverage Gates in CI

- Python coverage gate remains enforced with `PYTHON_COVERAGE_MIN` (currently `48`).
- Rust coverage gate remains enforced with `RUST_COVERAGE_MIN` (currently `30`).
- Both gates upload coverage artifacts and report to Codecov.

## Branch Protection Required Checks

Use these exact required status checks for `main` branch protection.

### Baseline required checks (always)

- `Enforce PR-only main updates`
- `Determine Impact`
- `Focused Gates`
- `Test`
- `Clippy`
- `Format`
- `Documentation`
- `Python Tests`
- `Rust Coverage`

### macOS enabled (`ENABLE_MACOS` not set to `false`)

- All baseline checks, plus:
  - `Test (macOS)`

### macOS disabled (`ENABLE_MACOS=false`)

- Baseline checks only.
- Remove `Test (macOS)` from required status checks.

### Checks to keep non-required (path/condition scoped)

- `Unsafe Compliance`
- `Protocol Contracts`
- `Python Quality`
- `Architecture Gate / Check ADR reference in PR body`
- `Crate Boundaries Gate / Require crate boundaries doc update for manifest changes`

These run when relevant and should remain non-required to avoid branch-protection deadlocks when
workflow path filters do not trigger.

### Admin runbook (`gh` CLI)

macOS enabled:

```bash
gh api --method PATCH repos/jmduea/NeuroHID/branches/main/protection/required_status_checks \
  -H "Accept: application/vnd.github+json" \
  -f strict=true \
  -f "contexts[]=Enforce PR-only main updates" \
  -f "contexts[]=Determine Impact" \
  -f "contexts[]=Focused Gates" \
  -f "contexts[]=Test" \
  -f "contexts[]=Test (macOS)" \
  -f "contexts[]=Clippy" \
  -f "contexts[]=Format" \
  -f "contexts[]=Documentation" \
  -f "contexts[]=Python Tests" \
  -f "contexts[]=Rust Coverage"
```

macOS disabled (`ENABLE_MACOS=false`):

```bash
gh api --method PATCH repos/jmduea/NeuroHID/branches/main/protection/required_status_checks \
  -H "Accept: application/vnd.github+json" \
  -f strict=true \
  -f "contexts[]=Enforce PR-only main updates" \
  -f "contexts[]=Determine Impact" \
  -f "contexts[]=Focused Gates" \
  -f "contexts[]=Test" \
  -f "contexts[]=Clippy" \
  -f "contexts[]=Format" \
  -f "contexts[]=Documentation" \
  -f "contexts[]=Python Tests" \
  -f "contexts[]=Rust Coverage"
```
