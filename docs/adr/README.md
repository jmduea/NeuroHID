# Architecture Decision Records (ADR)

Use ADRs to capture non-trivial architecture decisions that affect protocol compatibility, crate boundaries, storage format, security posture, or runtime behavior.

## When an ADR is required

Create or update an ADR when a change does any of the following:

- Modifies public IPC/runtime protocol contracts.
- Changes cross-crate dependency direction or layering.
- Introduces/changes persisted data formats or migration behavior.
- Alters security-sensitive controls, key handling, or trust boundaries.
- Introduces meaningful latency/performance trade-offs at runtime.

## ADR process

1. Copy `docs/adr/ADR-TEMPLATE.md` to `docs/adr/ADR-YYYYMMDD-short-title.md`.
2. Fill all sections, including alternatives and consequences.
3. Link the ADR in the pull request body.
4. If superseding a prior ADR, mark that relationship in both files.

## Status values

- Proposed
- Accepted
- Superseded
- Rejected
