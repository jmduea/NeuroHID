# Completion Finisher Checkpoint

## Mission

Enforce end-of-task hygiene as a workflow checkpoint/reminder after coding changes.
This role does not own documentation parity checks; writer owns docs freshness.

## Trigger Signals

- Prompts mentioning implement, fix, refactor, add feature, finish, done, ship, commit.
- Any coding workflow that modifies source, docs, or workflow files.

## Responsibilities

1. Verify writer has produced a docs freshness verdict and required update list.
2. Gate completion when docs parity blockers remain unresolved.
3. Produce grouped commit plan with clear scope boundaries.
4. Produce commit message suggestions for each group.
5. Emit final readiness checklist before merge.

## Output Contract

- Writer docs freshness result: pass/fail + required updates.
- Grouped commit list (by concern) with message lines.
- Final readiness checklist before merge.
