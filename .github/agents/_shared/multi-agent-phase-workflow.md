# Default Multi-Agent Phase Workflow

This file defines the canonical execution lifecycle for NeuroHID agent workflows.

## Phase Graph

1. Phase A: Discovery and planning
2. Phase B: Implementation
3. Phase C: Verification
4. Phase D: Documentation and completion hygiene

## Default Participants

- Discovery and planning: `.github/agents/deep-executor.md` (with its exploration/research handoffs)
- Rust specialization: `.github/agents/rust-skill-router.md`
- Verification: `.github/agents/verifier.md`
- Documentation freshness and updates: `.github/agents/writer.md`
- Completion checkpoint: `.github/agents/completion-finisher.md`

## Required Artifacts by Phase

### Phase A

- Scope interpretation and assumptions
- Target files and validation strategy

### Phase B

- Minimal implementation diff aligned with scope
- Focused validation after each increment

### Phase C

- Fresh verification evidence (tests/build/checks)
- Explicit PASS / FAIL / INCOMPLETE status

### Phase D

- Documentation freshness status (pass/fail)
- Required README/spec/changelog updates
- Grouped commit plan and message suggestions

## Route Precedence and Conflict Policy

- Domain-specific routes augment the default route, they do not replace it.
- If multiple domain routes match, all matched routes run.
- Rust domain signals must always include `.github/agents/rust-skill-router.md`.
- Completion hygiene must always include writer and completion-finisher.

## Stop Conditions

Work stops only when one of these is true:

- A required product decision is missing.
- A risky or destructive action needs explicit approval.
- No meaningful in-scope work remains.