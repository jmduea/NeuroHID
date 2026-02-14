# Agent & Skill Hygiene Report

Date: 2026-02-14
Scope: `.github/agents/*`, `.github/skills/*`, routing/docs contract integrity

## Summary

- Agent set was reduced to the actively utilized automation surface.
- Unused agent files were removed.
- Remaining agent definitions were normalized to role-relevant tool scopes and explicit handoff/subagent declarations where applicable.
- Skill docs were updated to remove dangling references to deleted agents and stale sample file paths.
- Routing and documentation contracts were re-validated after cleanup.

## Retained Agents (Current Active Set)

- `.github/agents/api-reviewer.md`
- `.github/agents/architect.md`
- `.github/agents/autonomy-execution-harness.md`
- `.github/agents/completion-finisher.md`
- `.github/agents/deep-executor.md`
- `.github/agents/designer.md`
- `.github/agents/explore.md`
- `.github/agents/planner.md`
- `.github/agents/product-manager.md`
- `.github/agents/researcher.md`
- `.github/agents/rust-skill-router.md`
- `.github/agents/scientist.md`
- `.github/agents/test-engineer.md`
- `.github/agents/ux-researcher.md`
- `.github/agents/verifier.md`
- `.github/agents/writer.md`

## Removed Agents

- `.github/agents/analyst.md`
- `.github/agents/browser-fetcher.md`
- `.github/agents/clippy-researcher.md`
- `.github/agents/code-reviewer.md`
- `.github/agents/crate-researcher.md`
- `.github/agents/critic.md`
- `.github/agents/debugger.md`
- `.github/agents/dependency-expert.md`
- `.github/agents/docs-cache.md`
- `.github/agents/docs-freshness.md`
- `.github/agents/docs-researcher.md`
- `.github/agents/executor.md`
- `.github/agents/git-master.md`
- `.github/agents/information-architect.md`
- `.github/agents/layer1-analyzer.md`
- `.github/agents/layer2-analyzer.md`
- `.github/agents/layer3-analyzer.md`
- `.github/agents/performance-reviewer.md`
- `.github/agents/product-analyst.md`
- `.github/agents/qa-tester.md`
- `.github/agents/quality-reviewer.md`
- `.github/agents/quality-strategist.md`
- `.github/agents/rust-changelog.md`
- `.github/agents/security-reviewer.md`
- `.github/agents/std-docs-researcher.md`
- `.github/agents/style-reviewer.md`
- `.github/agents/vision.md`

## Skill/Docs Reference Repairs

### Agent Reference Remaps

- `.github/skills/rust-learner/SKILL.md`
  - Replaced deleted specialist agent paths with `.github/agents/researcher.md`.
- `.github/skills/meta-cognition-parallel/SKILL.md`
  - Removed references to deleted layer analyzers.
  - Repointed parallel analysis examples to existing `.github/agents/architect.md` and `.github/agents/deep-executor.md`.
- `.github/skills/rust-daily/SKILL.md`
  - Replaced missing `rust-daily-reporter` agent reference with `.github/agents/researcher.md`.

### Non-Agent Path Fixes

- `.github/skills/core-fix-skill-docs/SKILL.md`
  - Replaced stale literal sample refs (`./references/file1.md`, etc.) with placeholder-safe `references/{module}.md` forms.
- `.github/skills/rust-refactor-helper/SKILL.md`
  - Replaced non-existent `docs/api.md` example with existing `docs/SPECIFICATION.md`.
- `.github/skills/rust-skill-creator/SKILL.md`
  - Replaced stale literal `./references/overview.md` sample with placeholder-safe `references/{topic}.md`.

## Validation Evidence

- `pwsh -File .github/hooks/validate-routing.ps1` → PASS
- `pwsh -File .github/hooks/validate-doc-contracts.ps1` → PASS
- Repo-wide deleted-agent reference sweep → no matches
- Strict skill path existence sweep (extension-based path refs) → no broken references

## Notes

- Cleanup intentionally preserved the hook-routed/default workflow agent surface defined in:
  - `AGENTS.md`
  - `.github/hooks/hooks.json`
  - `.github/hooks/TRIGGERS.md`
  - `.github/agents/_shared/multi-agent-phase-workflow.md`
  - `docs/automation/agent-skill-invocation-playbook.md`
