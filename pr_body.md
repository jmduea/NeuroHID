# Pull Request Summary

## Summary

- Add canonical governance policy manifest for branch/TDD enforcement.
- Add CI governance integrity workflow + validator script.
- Add PR TDD governance workflow + validator script with `no-test-impact` auto-label hinting.
- Add local governance helper scripts for setup verification and pre-push checks.
- Add fixture-based regression tests for policy validator (missing-path + forbidden-claim).
- Align agent/contributor/development docs and PR template with enforced governance.

## Scope

- [x] In scope and out-of-scope are clear.

## Governance Checks

- [x] I updated documentation impacted by this change.
- [x] I confirmed no generated/local artifacts are committed (`target/`, coverage outputs, local logs).
- [x] I updated `CHANGELOG.md` when behavior or public surface changed.
- [x] I reviewed impact classification outputs (rust/python/docs/protocol/unsafe/architecture) and covered required gates.
- [x] I evaluated whether an ADR is required.
- [x] If required, I linked an ADR in `docs/adr/`.
- [x] I ran a docs-freshness pass and resolved blockers.
- [x] I refreshed `docs/architecture/index.md` when architecture-gated surfaces changed.
- [x] I updated `docs/crate-boundaries.md` when crate ownership or dependency direction changed.

## Testing / TDD

- [x] I added or updated tests for behavior changes.
- [x] Rust checks pass for affected crates.
- [x] Python checks pass for affected modules.
- [x] Protocol contract checks pass when protocol/runtime types changed.
- [x] Unsafe compliance checks pass when unsafe code paths changed.

### Failing Test Intent

Ensure governance-integrity validation fails when `CHANGELOG.md` Unreleased entries introduce missing local references or forbidden stale governance claims, while valid references still pass.

### RED Evidence (Before)

- Command(s) run: `pwsh -File ./.github/scripts/test-validate-policy-integrity.ps1`
- Failure summary: fixture harness initially failed due missing fixture path setup and signal-capture bugs; failing case did not reliably assert missing-reference and forbidden-claim branches.

### GREEN Evidence (After)

- Command(s) rerun:
  - `pwsh -File ./.github/scripts/test-validate-policy-integrity.ps1`
  - `pwsh -File ./.github/scripts/validate-policy-integrity.ps1`
  - `pwsh -File ./.github/scripts/verify-governance-setup.ps1`
- Passing summary: fixture regression suite passes (including missing-path + forbidden-claim failures), policy integrity validator passes, setup verifier passes on feature branch.

### No-Test-Impact Rationale

Not used. This change introduces/updates governance validation scripts and fixture-based tests directly.

## Risk and Rollout

- [x] I documented migration/compatibility implications.
- [x] I documented rollback/fallback plan if risk is non-trivial.

## Commit Hygiene

- [x] I grouped commits by concern (code/tests/docs/ci).
- [x] I used clear commit messages for each commit group.


ADR: docs/adr/ADR-0001-governance-ci-enforcement.md
