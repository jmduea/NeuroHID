---
name: completion-finisher
description: Require docs-freshness checks and grouped well-documented commit output at task completion.
user-invocable: true
---

# Skill: completion-finisher

## Purpose

Standardize task completion for coding work.

## Inputs

- Changed files list.
- User-facing behavior and protocol changes.
- Test and docs update status.

## Checks

1. Docs-freshness has been run after coding changes.
2. Required README/spec/changelog updates are complete.
3. Commit groups are logically scoped and non-overlapping.
4. Each commit message is clear and audit-friendly.

## Output

- Docs-freshness pass/fail.
- Required doc updates.
- Grouped commit plan and message suggestions.
