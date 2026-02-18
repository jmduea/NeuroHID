# Pull Request Summary

## Summary

-

## Scope

- [ ] In scope and out-of-scope are clear.

## Governance Checks

- [ ] I updated documentation impacted by this change.
- [ ] I confirmed no generated/local artifacts are committed (`target/`, coverage outputs, local logs).
- [ ] I updated `CHANGELOG.md` when behavior or public surface changed.
- [ ] I reviewed impact classification outputs (rust/python/docs/protocol/unsafe/architecture) and covered required gates.
- [ ] I evaluated whether an ADR is required.
- [ ] If required, I linked an ADR in `docs/adr/`.
- [ ] I ran a docs-freshness pass and resolved blockers.
- [ ] I refreshed `docs/architecture/index.md` when architecture-gated surfaces changed.
- [ ] I updated `docs/crate-boundaries.md` when crate ownership or dependency direction changed.

## Testing / TDD

- [ ] I added or updated tests for behavior changes.
- [ ] Rust checks pass for affected crates.
- [ ] Python checks pass for affected modules.
- [ ] Protocol contract checks pass when protocol/runtime types changed.
- [ ] Unsafe compliance checks pass when unsafe code paths changed.

### Failing Test Intent

Describe the behavior gap that failed first and what requirement it covers.

### RED Evidence (Before)

- Command(s) run:
- Failure summary:

### GREEN Evidence (After)

- Command(s) rerun:
- Passing summary:

### No-Test-Impact Rationale

Only use when production code changed but dedicated test files were not updated.
Include why test files were not required and apply label `no-test-impact`.
When rationale is substantive, CI may auto-apply the `no-test-impact` label.

## Risk and Rollout

- [ ] I documented migration/compatibility implications.
- [ ] I documented rollback/fallback plan if risk is non-trivial.

## Commit Hygiene

- [ ] I grouped commits by concern (code/tests/docs/ci).
- [ ] I used clear commit messages for each commit group.
