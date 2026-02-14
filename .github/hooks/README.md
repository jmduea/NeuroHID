# NeuroHID Hook Routing

This directory contains NeuroHID-specific hook routing.

- Repo-specific workflow routing lives in `.github/hooks/hooks.json`.

## Event Contract

Current manifest defines `UserPromptSubmit` matchers and routes to BMAD agent IDs.

- Routing contract version: `2026-02-v1`
- Schema: `.github/hooks/hooks.schema.json`

Default orchestration follows:

- `_bmad/neurohid/workflows/neurohid-phase-workflow/workflow.md`

## Maintenance

- Keep matchers minimal and deterministic.
- Prefer updating agent logic over broadening regex matchers.
- Review false positives monthly.
- Validate routing with `.github/hooks/validate-routing.ps1`.
- Run fixture regression checks with `.github/hooks/test-validate-routing.ps1`.
- Validate docs vocabulary and canonical links with `.github/hooks/validate-doc-contracts.ps1`.
- Run all harness checks + prompt route matrix with `.github/hooks/run-harness-smoke.ps1`.
- Classify changed-file impact with `.github/scripts/classify-impact.ps1`.
- Run canonical local quality sequence with `.github/scripts/run-agent-ready-tasks.ps1`.
- Keep scope routing map fresh in `.github/automation/scope-map.json`.
- Keep catch-all matcher `(?s).+` as a single final route.
- Keep hook-routed agents documented in `AGENTS.md`, `.github/hooks/TRIGGERS.md`, and `docs/automation/agent-skill-invocation-playbook.md`.
