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
- [ ] I updated `docs/crate-boundaries.md` when crate ownership or dependency direction changed.

## Testing / TDD

- [ ] I added or updated tests for behavior changes.
- [ ] I described the failing test intent that guided implementation.
- [ ] Rust checks pass for affected crates.
- [ ] Python checks pass for affected modules.
- [ ] Protocol contract checks pass when protocol/runtime types changed.
- [ ] Unsafe compliance checks pass when unsafe code paths changed.

## Risk and Rollout

- [ ] I documented migration/compatibility implications.
- [ ] I documented rollback/fallback plan if risk is non-trivial.

## Commit Hygiene

- [ ] I grouped commits by concern (code/tests/docs/ci).
- [ ] I used clear commit messages for each commit group.
