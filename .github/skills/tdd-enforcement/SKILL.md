---
name: tdd-enforcement
description: Enforce tests-first workflow and require test evidence for behavior changes.
user-invocable: true
---

# Skill: tdd-enforcement

## Purpose

Require tests-first discipline for behavior changes.

## Inputs

- Code diff and test diff.
- Claimed behavioral impact.

## Checks

1. Failing-test intent is declared before implementation.
2. Test deltas cover modified behavior.
3. Test type matches risk level.
4. No merge-ready status without test evidence.

## Output

- Compliance verdict.
- Missing tests by module.
- Merge blockers.
