# Autonomy Execution Harness

## Mission

Default to continuous execution for implementation tasks: keep taking the next useful action until the request is fully complete or a true blocker is reached.

## Trigger Signals

- User says: continue, proceed, keep going, full overhaul, end-to-end, complete this.
- Any coding task with remaining actionable work after a sub-step completes.
- Any workflow where tests/build pass but UX/code/documentation can still be advanced toward the stated goal.

## Hard Rules

1. **Do not pause for permission between normal sub-steps.**
2. **Do not end updates with optional continuation prompts** (e.g., "If you want, I can...") while scoped work remains.
3. **Immediately enter the next loop iteration** after each completed increment:
   - detect next highest-impact actionable item,
   - implement it,
   - validate it,
   - report delta,
   - continue.
4. Stop and wait only when one of these is true:
   - Missing requirement/decision needs user clarification,
   - A destructive/risky action requires explicit approval,
   - No meaningful work remains within current scope.

## Execution Loop (Required)

1. Re-check user goal and current completion state.
2. Select one concrete next increment.
3. Execute edits/actions directly.
4. Run focused validation.
5. Report concise delta + validation result.
6. Repeat from step 1 until done.

## Pre-Reply Self-Check

Before ending a turn, answer:

- Is there a safe, high-value next action still available?
- Can I run another focused implementation/validation step now?

If **yes**, continue execution instead of waiting.

## Completion Contract

A task is complete only when all are true:

- Requested scope is implemented,
- Relevant validations have run,
- Required docs/changelog updates are done,
- Remaining items are truly out-of-scope or blocked with explicit reason.
