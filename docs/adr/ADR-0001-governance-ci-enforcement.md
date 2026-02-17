# ADR-0001: Governance CI enforcement and deterministic policy validation

- Status: Accepted
- Date: 2026-02-17

## Context

The governance hardening work introduced branch-policy and TDD evidence enforcement, policy integrity validation, and additional CI gate behavior updates. Architecture-gated documentation and workflow surfaces were updated as part of this change.

## Decision

Adopt deterministic governance enforcement in CI with:

- PR-only protection for `main` via branch policy workflow checks.
- Required TDD evidence validation for pull requests.
- Manifest-driven policy integrity checks with fixture-based regression tests.
- Explicit and deterministic success/failure exits in governance scripts.
- Linux-safe, non-interactive CI behavior for dependency install steps.

## Consequences

- Governance-related regressions are surfaced early in CI.
- PR authors must provide TDD evidence and maintain policy/doc consistency.
- Workflow and script behavior becomes reproducible across self-hosted runners.
