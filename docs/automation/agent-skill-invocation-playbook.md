# Agent + Skill Invocation Playbook

Use these prompts directly in chat to trigger the current NeuroHID automation path.

Related maintenance reference:

- `docs/automation/agent-skill-hygiene-report.md`

## 0) Canonical Local/CI Entry Points

- Impact classifier: `.github/scripts/classify-impact.ps1`
- Scope map: `.github/automation/scope-map.json`
- Canonical quality runner: `.github/scripts/run-agent-ready-tasks.ps1`
- Docs freshness validator: `.github/scripts/check-docs-freshness.ps1`
- Unsafe validator: `.github/scripts/check-unsafe-compliance.ps1`
- Protocol validator: `.github/scripts/verify-protocol-contracts.ps1`

Recommended local command:

- `pwsh -File ./.github/scripts/run-agent-ready-tasks.ps1 -RustScope focused -WithDocs -WithProtocol -WithUnsafe`

## 0.1) BMAD-First Replacement Policy for `.github`

Use BMAD as the implementation owner for all applicable components while preserving required GitHub platform boundaries.

Replaceable (migrate to BMAD-owned assets first):

- `.github/prompts/bmad-*` prompt/task logic → `_bmad/*/workflows/*` and `_bmad/*/agents/*`
- `.github` process docs that describe BMAD flows → `docs/automation/*` and `_bmad/neurohid/*`

Keep (platform/shared boundary, not BMAD-replaceable in current architecture):

- `.github/workflows/*` (GitHub Actions event + checks integration)
- `.github/hooks/*` (hook wiring contract)
- `.github/skills/*` (shared Rust/domain skill registry)
- `.github/PULL_REQUEST_TEMPLATE.md` (GitHub PR UI template)

Hybrid (keep thin wrappers, move logic to BMAD workflows):

- `.github/scripts/classify-impact.ps1`
- `.github/scripts/generate-architecture-index.ps1`

Canonical matrix:

- `docs/automation/github-to-bmad-replacement-matrix.md`

## 1) Default Multi-Agent Coordination (Always On)

Canonical phase workflow:

- `_bmad/neurohid/workflows/neurohid-phase-workflow/workflow.md`

Default agents for every prompt:

- `deep-executor`
- `verifier`
- `writer`
- `completion-finisher`

Prompt example:

- `Run the default multi-agent phase workflow and continue until completion or a true blocker.`

---

## 2) Documentation Freshness + Docs Updates (Writer-Owned)

Agent:

- `writer`

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

- `architect`
- `api-reviewer`

Prompt example:

- `Run architecture and API compatibility review for this diff; identify ADR, migration, and compatibility requirements.`

Workflow:

1. Run architecture/API review.
2. If required, update `docs/adr/*` and migration notes.
3. Re-check docs freshness with writer.

---

## 4) Planning, TDD, and UX Specialization

Planning agents:

- `product-manager`
- `planner`

TDD/verification agents:

- `test-engineer`
- `verifier`

UX agents:

- `ux-researcher`
- `designer`

Prompt examples:

- `Generate implementation-ready scope and acceptance criteria for this feature.`
- `Run TDD and verification coverage review for this behavior change.`
- `Run UX and accessibility review for this user-facing change.`

---

## 5) Rust Skill Routing + Canonical Grounding

Rust router agent:

- `rust-skill-router`

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

- `completion-finisher`

Prompt example:

- `Run completion-finisher checkpoint: verify writer docs freshness output and produce grouped commit messages.`

Workflow:

1. Ensure implementation and verification evidence are complete.
2. Ensure writer docs freshness output is PASS or blockers are explicit.
3. Produce grouped commit plan and commit message suggestions.
