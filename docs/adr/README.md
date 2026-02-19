# Architecture Decision Records (ADRs)

This directory contains Architecture Decision Records for the NeuroHID project.

## When to Write an ADR

An ADR is required when a PR modifies architecture-gated paths:

- `crates/neurohid-ipc/**`
- `crates/neurohid-storage/**`
- `crates/neurohid-core/**`
- `docs/protocol-and-api.md`

The CI `architecture-gate` workflow checks that the PR body references an ADR
(e.g., `docs/adr/ADR-001-...`).

## Naming Convention

```
ADR-NNN-short-title.md
```

Use the next available number. Pad to three digits.

## Template

Use [ADR-000-template.md](ADR-000-template.md) as a starting point.

## Status Values

- **Proposed** — Under discussion, not yet accepted.
- **Accepted** — Approved and in effect.
- **Superseded** — Replaced by a newer ADR (link to successor).
- **Deprecated** — No longer relevant.
