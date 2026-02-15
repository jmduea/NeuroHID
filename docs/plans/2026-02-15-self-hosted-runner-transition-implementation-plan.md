# Self-Hosted Runner Transition Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Move private-phase CI/CD to self-hosted runners while preserving PR quality gates and coverage enforcement, with a simple release-time switch to GitHub-hosted runners.

**Architecture:** Workflows use explicit self-hosted runner label arrays. Linux policy jobs run on dedicated Linux labels, cross-platform tests run Linux/Windows plus optional macOS, and macOS can be disabled through repository variable `ENABLE_MACOS`.

**Tech Stack:** GitHub Actions YAML, PowerShell/Bash CI scripts, Rust (`cargo`), Python (`uv`, `pytest-cov`), Codecov.

---

## Task 1: Route branch policy to self-hosted

**Files:**

- Modify: `.github/workflows/branch-policy.yml`

- Step 1: Define check

- Workflow parse succeeds and job uses self-hosted Linux labels.

- Step 2: Verify baseline

- Run grep for hosted `runs-on` labels in the file.

- Step 3: Implement

- Set `runs-on: [self-hosted, linux, neurohid-ci]`.

- Step 4: Validate

- Confirm workflow parses with no errors.

## Task 2: Route policy/quality workflows to self-hosted Linux

**Files:**

- Modify: `.github/workflows/architecture-gate.yml`
- Modify: `.github/workflows/crate-boundaries-gate.yml`
- Modify: `.github/workflows/python-quality.yml`

- Step 1: Define check

- Each workflow uses explicit Linux self-hosted labels.

- Step 2: Verify baseline

- Search for fixed `ubuntu-latest` entries.

- Step 3: Implement

- Replace each `runs-on` with `[self-hosted, linux, neurohid-ci]`.

- Step 4: Validate

- Confirm workflow syntax and no residual hosted Linux labels.

## Task 3: Route CI workflow and preserve gates

**Files:**

- Modify: `.github/workflows/ci.yml`

- Step 1: Define check

- CI jobs keep gate behavior and coverage enforcement.

- Step 2: Verify baseline

- Confirm current coverage vars and commands exist.

- Step 3: Implement

- Route Linux jobs to `[self-hosted, linux, neurohid-ci]`.
- Route Windows matrix lane to `[self-hosted, windows, neurohid-ci]`.
- Add macOS job on `[self-hosted, macos, neurohid-ci]` behind `vars.ENABLE_MACOS != 'false'`.

- Step 4: Validate

- Confirm no parser errors.
- Confirm `PYTHON_COVERAGE_MIN`, `RUST_COVERAGE_MIN`, `cov-fail-under`, `fail-under-lines` still exist.

## Task 4: Route release/publish workflows

**Files:**

- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/publish-crates.yml`

- Step 1: Define check

- Tagged verification and crates publishing run on self-hosted Linux.

- Step 2: Verify baseline

- Search for hosted labels.

- Step 3: Implement

- Set `runs-on: [self-hosted, linux, neurohid-ci]` in both files.

- Step 4: Validate

- Confirm syntax and no hosted Linux labels remain.

## Task 5: Update documentation

**Files:**

- Modify: `docs/development-guide.md`
- Modify: `CHANGELOG.md`

- Step 1: Define check

- Docs reflect private-phase self-hosted policy and macOS off-ramp.

- Step 2: Implement

- Add runner policy, macOS toggle variable guidance, and coverage-gate continuity notes.

- Step 3: Validate

- Markdown lint and manual consistency read.

## Task 6: Final verification

**Files:**

- Validate: `.github/workflows/*.yml`

- Step 1: Commands

- `rg "runs-on:" .github/workflows`
- `rg "PYTHON_COVERAGE_MIN|RUST_COVERAGE_MIN|cov-fail-under|fail-under-lines" .github/workflows`

- Step 2: Expected

- Workflows target self-hosted labels for private phase.
- Coverage gates remain enforced.
