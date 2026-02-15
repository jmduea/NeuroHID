# Self-Hosted Runner Transition Design

Date: 2026-02-15  
Status: Approved

## Goal

Run CI/CD on self-hosted GitHub Actions runners while the repository is private, with a low-risk switch back to GitHub-hosted runners at release time.

## Constraints

- Repository remains private until release.
- Prefer cross-platform parity (Linux, Windows, macOS).
- macOS support may be dropped if maintenance burden is disproportionate.
- Existing quality and coverage gates must remain enforced on pull requests and main pushes.

## Decision

Adopt a hybrid-by-lifecycle strategy:

1. Private phase: self-hosted runners for all workflows.
2. Release/public phase: switch runner label mappings to GitHub-hosted labels.

## Runner Policy

Use explicit runner labels to avoid accidental scheduling:

- Linux: `[self-hosted, linux, neurohid-ci]`
- Windows: `[self-hosted, windows, neurohid-ci]`
- macOS: `[self-hosted, macos, neurohid-ci]`

Each workflow uses explicit runner label arrays directly in `runs-on`.

## macOS Off-Ramp

`ci.yml` checks repository variable `ENABLE_MACOS`.

- unset/any value except `false`: includes macOS lane.
- `false`: skips macOS lane without changing Linux/Windows lanes.

If macOS is disabled long-term, required-check policy in GitHub branch protection should be updated to match.

## Quality and Coverage Guarantees

Keep pre-merge validation unchanged in behavior:

- Rust tests/clippy/fmt/docs gates still run on PR/push.
- Python quality workflow still runs on PR/push for Python paths.
- Coverage minimums remain enforced:
  - Python: `PYTHON_COVERAGE_MIN = 48`
  - Rust: `RUST_COVERAGE_MIN = 30`

## Release Transition Procedure

At release/public transition, update runner mappings from self-hosted labels to GitHub-hosted labels:

- Linux mapping to `["ubuntu-latest"]`
- Windows mapping to `["windows-latest"]`
- macOS mapping to `["macos-latest"]`

No job logic changes required.

## Risk and Mitigations

- Runner drift: pin toolchain setup in workflows and keep host bootstrap docs/versioning.
- Availability: maintain at least one healthy runner per required label.
- Security: keep self-hosted labels dedicated to this repository and avoid broad sharing.
