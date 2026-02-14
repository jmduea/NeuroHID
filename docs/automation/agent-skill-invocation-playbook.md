# Agent + Skill Invocation Playbook

Use these prompts directly in chat to trigger the current NeuroHID automation path.

## 1) Default Multi-Agent Coordination (Always On)

Canonical phase workflow:

- `.github/agents/_shared/multi-agent-phase-workflow.md`

Default agents for every prompt:

- `.github/agents/deep-executor.md`
- `.github/agents/verifier.md`
- `.github/agents/writer.md`
- `.github/agents/completion-finisher.md`

Prompt example:

- `Run the default multi-agent phase workflow and continue until completion or a true blocker.`

---

## 2) Documentation Freshness + Docs Updates (Writer-Owned)

Agent:

- `.github/agents/writer.md`

Skills:

- `.github/skills/docs-freshness/SKILL.md`
- `.github/skills/completion-finisher/SKILL.md`

Prompt example:

- `Run writer docs freshness review for this change and list required README/spec/changelog updates with blockers.`

Workflow:

1. Implement or verify behavior changes.
2. Run writer docs freshness review.
3. Apply required `README.md` / `docs/*` / `CHANGELOG.md` updates.
4. Re-run writer docs freshness review until no blockers remain.

---

## 3) Architecture + Compatibility Review

Agents:

- `.github/agents/architect.md`
- `.github/agents/api-reviewer.md`

Prompt example:

- `Run architecture and API compatibility review for this diff; identify ADR, migration, and compatibility requirements.`

Workflow:

1. Run architecture/API review.
2. If required, update `docs/adr/*` and migration notes.
3. Re-check docs freshness with writer.

---

## 4) Planning, TDD, and UX Specialization

Planning agents:

- `.github/agents/product-manager.md`
- `.github/agents/planner.md`

TDD/verification agents:

- `.github/agents/test-engineer.md`
- `.github/agents/verifier.md`

UX agents:

- `.github/agents/ux-researcher.md`
- `.github/agents/designer.md`

Prompt examples:

- `Generate implementation-ready scope and acceptance criteria for this feature.`
- `Run TDD and verification coverage review for this behavior change.`
- `Run UX and accessibility review for this user-facing change.`

---

## 5) Rust Skill Routing + Canonical Grounding

Rust router agent:

- `.github/agents/rust-skill-router.md`

Primary skill router:

- `.github/skills/rust-router/SKILL.md`

Canonical Rust sources for tier-2 escalation:

- Rust Book: <https://doc.rust-lang.org/book/>
- Rust Reference: <https://doc.rust-lang.org/stable/reference/>
- Cargo Book: <https://doc.rust-lang.org/stable/cargo/>
- Effective Rust: <https://effective-rust.com/>

Prompt example:

- `Route this Rust issue through rust-router and cite canonical sources for any disputed or safety-critical guidance.`

---

## 6) Completion Checkpoint (After Coding)

Checkpoint role:

- `.github/agents/completion-finisher.md`

Prompt example:

- `Run completion-finisher checkpoint: verify writer docs freshness output and produce grouped commit messages.`

Workflow:

1. Ensure implementation and verification evidence are complete.
2. Ensure writer docs freshness output is PASS or blockers are explicit.
3. Produce grouped commit plan and commit message suggestions.
