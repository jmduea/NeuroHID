# Agent + Skill Invocation Playbook

Use these prompts directly in chat to trigger the right NeuroHID automation path.

## 1) Documentation Freshness

### Documentation Freshness Agent

- Agent file: `.github/agents/docs-freshness.md`
- Prompt:
  - `Run docs-freshness review for this change. Identify every README/spec/changelog update required before merge.`

### Documentation Freshness Skill

- Skill file: `.github/skills/docs-freshness/SKILL.md`
- Prompt:
  - `Apply docs-freshness skill: map changed code/protocol files to required documentation updates and list blockers.`

### Documentation Freshness Workflow

1. Implement code or protocol changes.
2. Run docs-freshness prompt.
3. Update `README.md`, `docs/*`, and `CHANGELOG.md` as required.
4. Re-run docs-freshness until no blockers remain.

---

## 2) Architecture Validation (ADR)

### Architecture Validation Agent

- Agent file: `.github/agents/architecture-validator.md`
- Prompt:
  - `Run architecture-validator on this diff. Tell me if ADR is required and what compatibility or migration notes are mandatory.`

### Architecture Validation Skill

- Skill file: `.github/skills/architecture-validator/SKILL.md`
- Prompt:
  - `Apply architecture-validator skill to assess boundary/layering impact and produce ADR requirement rationale.`

### Architecture Validation Workflow

1. Draft or review architecture-impacting changes.
2. Run architecture-validator prompt.
3. If required, create/update ADR in `docs/adr/`.
4. Add migration/compatibility notes and link ADR in PR body.

---

## 3) Feature Planning

### Feature Planning Agent

- Agent file: `.github/agents/feature-planner.md`
- Prompt:
  - `Use feature-planner to turn this feature request into implementation-ready scope with acceptance criteria, TDD test intent, and rollout steps.`

### Feature Planning Skill

- Skill file: `.github/skills/feature-planning/SKILL.md`
- Prompt:
  - `Apply feature-planning skill and produce a filled feature brief plus DoR/DoD status.`

### Feature Planning Workflow

1. Start from issue/feature request.
2. Generate feature brief using planner prompt.
3. Validate DoR in `docs/planning/definition-of-ready.md`.
4. Execute in slices; close with DoD in `docs/planning/definition-of-done.md`.

---

## 4) TDD Enforcement

### TDD Enforcement Agent

- Agent file: `.github/agents/tdd-enforcer.md`
- Prompt:
  - `Run tdd-enforcer: require failing-test intent first, then verify test deltas fully cover this behavior change.`

### TDD Enforcement Skill

- Skill file: `.github/skills/tdd-enforcement/SKILL.md`
- Prompt:
  - `Apply tdd-enforcement skill and report missing tests by module and merge blockers.`

### TDD Enforcement Workflow

1. Describe expected behavior change.
2. Write/identify failing test first.
3. Implement minimal code change.
4. Re-run tests and tdd-enforcer prompt.
5. Merge only when no test gaps remain.

---

## 5) UX/UI Review

### UX/UI Review Agent

- Agent file: `.github/agents/ux-reviewer.md`
- Prompt:
  - `Run ux-reviewer for this change across app UX, docs UX, and notebook UX using docs/ux/interaction-checklist.md.`

### UX/UI Review Skill

- Skill file: `.github/skills/ux-ui-review/SKILL.md`
- Prompt:
  - `Apply ux-ui-review skill and return required fixes by severity.`

### UX/UI Review Workflow

1. Complete a user-facing change.
2. Run ux-reviewer prompt.
3. Fix high-severity issues first.
4. Confirm state coverage (loading/success/empty/error) and accessibility basics.

---

## 6) Python ML Specialist

### Python ML Specialist Agent

- Agent file: `.github/agents/python-ml-specialist.md`
- Prompt:
  - `Run python-ml-specialist review for this ML change: validate protocol compatibility, reproducibility, and required quality gates.`

### Python ML Specialist Skill

- Skill file: `.github/skills/python-ml-specialist/SKILL.md`
- Prompt:
  - `Apply python-ml-specialist skill and report integration risks, missing validation steps, and blockers.`

### Python ML Specialist Workflow

1. Update Python ML/runtime/notebook code.
2. Run python-ml-specialist prompt.
3. Run uv-only checks: `uv run --project python ruff check python/src python/tests`, `uv run --project python black --check python/src python/tests`, `uv run --project python mypy python/src`, `uv run --project python pytest python/tests -q`.
4. Confirm protocol/docs alignment and close blockers.

---

## 7) CI/CD Optimization

No dedicated CI/CD agent exists yet. Use this prompt until one is added:

- Prompt:
  - `Review this workflow or pipeline change for CI/CD hardening: path filters, gate relevance, failure quality, and release safety.`

Typical workflow:

1. Modify `.github/workflows/*`.
2. Run the CI/CD review prompt.
3. Validate only relevant gates trigger.
4. Confirm release safety and rollback clarity.

---

## 8) Completion Finisher (Always after coding)

### Completion Finisher Agent

- Agent file: `.github/agents/completion-finisher.md`
- Prompt:
  - `Run completion-finisher now: execute docs-freshness checks and produce grouped commits with clear commit messages.`

### Completion Finisher Skill

- Skill file: `.github/skills/completion-finisher/SKILL.md`
- Prompt:
  - `Apply completion-finisher skill to confirm docs are fresh and output grouped commit plan + commit messages.`

### Completion Finisher Workflow

1. Finish implementation and tests.
2. Run docs-freshness review.
3. Apply required README/spec/changelog updates.
4. Generate grouped commit plan (code/tests/docs/ci).
5. Generate clear commit messages per group.

---

## 9) Autonomy Execution Harness (Run first on execution tasks)

### Autonomy Harness Agent

- Agent file: `.github/agents/autonomy-execution-harness.md`
- Prompt:
  - `Run autonomy-execution-harness and continue implementing in a loop until the current request is complete or truly blocked. Do not pause for permission between normal sub-steps.`

### Autonomy Harness Workflow

1. Start execution task.
2. Run autonomy harness prompt.
3. Continue implementation/validation loop without waiting for "continue/proceed" prompts.
4. Stop only for clarification, required approval, or true no-work-left state.
5. Then run completion-finisher flow.
