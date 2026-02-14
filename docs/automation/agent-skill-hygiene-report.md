# Agent & Skill Hygiene Report

Date: 2026-02-14
Scope: BMAD agent IDs, `.github/skills/*`, routing/docs contract integrity

## Summary

- Agent set was reduced to the actively utilized automation surface.
- Unused agent files were removed.
- Remaining agent definitions were normalized to role-relevant tool scopes and explicit handoff/subagent declarations where applicable.
- Skill docs were updated to remove dangling references to deleted agents and stale sample file paths.
- Routing and documentation contracts were re-validated after cleanup.

## Retained Agents (Current Active Set)

- `api-reviewer`
- `architect`
- `autonomy-execution-harness` (legacy compat role)
- `completion-finisher`
- `deep-executor`
- `designer`
- `explore`
- `planner`
- `product-manager`
- `researcher`
- `rust-skill-router`
- `scientist`
- `test-engineer`
- `ux-researcher`
- `verifier`
- `writer`

## Removed Agents

- `analyst`
- `browser-fetcher`
- `clippy-researcher`
- `code-reviewer`
- `crate-researcher`
- `critic`
- `debugger`
- `dependency-expert`
- `docs-cache`
- `docs-freshness`
- `docs-researcher`
- `executor`
- `git-master`
- `information-architect`
- `layer1-analyzer`
- `layer2-analyzer`
- `layer3-analyzer`
- `performance-reviewer`
- `product-analyst`
- `qa-tester`
- `quality-reviewer`
- `quality-strategist`
- `rust-changelog`
- `security-reviewer`
- `std-docs-researcher`
- `style-reviewer`
- `vision`

## Skill/Docs Reference Repairs

### Agent Reference Remaps

- `.github/skills/rust-learner/SKILL.md`
  - Replaced deleted specialist agent paths with `researcher`.
- `.github/skills/meta-cognition-parallel/SKILL.md`
  - Removed references to deleted layer analyzers.
  - Repointed parallel analysis examples to existing `architect` and `deep-executor`.
- `.github/skills/rust-daily/SKILL.md`
  - Replaced missing `rust-daily-reporter` agent reference with `researcher`.

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
  - `_bmad/neurohid/workflows/neurohid-phase-workflow/workflow.md`
  - `docs/automation/agent-skill-invocation-playbook.md`
