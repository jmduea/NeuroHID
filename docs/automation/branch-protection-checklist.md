# Branch Protection Checklist

Use this checklist when configuring repository rules for `main`.

## Required baseline checks

- `Enforce PR-only main updates`
- `Determine Impact`
- `Focused Gates`
- `Test`
- `Clippy`
- `Format`
- `Documentation`
- `Python Tests`
- `Rust Coverage`
- `Governance Integrity`
- `TDD Evidence`

## Conditional checks

- Add `Test (macOS)` when repository variable `ENABLE_MACOS` is not set to `false`.

## Required branch settings

- Require a pull request before merging.
- Require status checks to pass before merging.
- Require branches to be up to date before merging.
- Restrict force pushes and deletions on `main`.
- Keep direct pushes to `main` disabled for development flow.

## Source of truth

- Governance policy manifest: `.github/automation/policy-manifest.json`
- Operational runbook and command examples: `docs/development-guide.md`
