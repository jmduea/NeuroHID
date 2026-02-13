---
name: architecture-validator
description: Validate architecture-impacting changes and enforce ADR linkage and compatibility notes.
user-invocable: true
---

# Skill: architecture-validator

## Purpose

Assess architectural impact and ADR requirements.

## Inputs

- Cross-crate API changes.
- IPC/storage/protocol edits.
- New dependencies or layer direction changes.

## Checks

1. Layering and dependency direction remain valid.
2. Compatibility strategy is explicit.
3. ADR required conditions are evaluated.
4. Migration and rollback implications are documented.

## Output

- ADR requirement and rationale.
- Architectural risk summary.
- Required follow-up actions.
