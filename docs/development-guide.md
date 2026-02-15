# Development Guide

## Prerequisites

- Rust `1.85+`
- Python `3.12+`
- `uv` for Python environment and command execution

## Local Setup

```bash
cargo build --workspace
uv sync --directory python
```

## Common Run Commands

```bash
cargo run -p neurohid --bin neurohid
cargo run -p neurohid --bin neurohid-service
uv run --directory python neurohid-ml bridge
```

## Validation and Testing

Rust:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Python:

```bash
uv run --project python pytest python/tests -q
uv run --project python ruff check python/src
uv run --project python black --check python/src
uv run --project python mypy python/src
```

## CI Runner Policy

- Private phase uses self-hosted GitHub Actions runners with dedicated labels:
  - Linux: `self-hosted`, `linux`, `neurohid-ci`
  - Windows: `self-hosted`, `windows`, `neurohid-ci`
  - macOS: `self-hosted`, `macos`, `neurohid-ci`
- `ci.yml` controls optional macOS execution with repository variable `ENABLE_MACOS`.
- If macOS support is dropped temporarily, set `ENABLE_MACOS=false` in repo variables and align required checks in branch protection.
- At release/public transition, change workflow runner mappings to GitHub-hosted labels (`ubuntu-latest`, `windows-latest`, `macos-latest`).

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

These run when relevant and should remain non-required to avoid branch-protection deadlocks when workflow path filters do not trigger.

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

## Automation Scripts

Repository scripts under `.github/scripts/` support focused rust/python/doc/unsafe gates and
architecture-index generation.
