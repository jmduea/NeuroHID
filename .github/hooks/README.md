# NeuroHID Hook Routing

This directory contains NeuroHID-specific hook routing.

- Shared cross-project Rust hook behavior remains in `.rust-skills/hooks/hooks.json`.
- Repo-specific workflow routing lives in `.github/hooks/hooks.json`.

## Event Contract

Current manifest defines `UserPromptSubmit` matchers and routes to `.github/agents/*` assets.

- Routing contract version: `2026-02-v1`
- Schema: `.github/hooks/hooks.schema.json`

Default orchestration follows:

- `.github/agents/_shared/multi-agent-phase-workflow.md`

## Maintenance

- Keep matchers minimal and deterministic.
- Prefer updating agent logic over broadening regex matchers.
- Review false positives monthly.
- Validate routing with `.github/hooks/validate-routing.ps1`.
- Run fixture regression checks with `.github/hooks/test-validate-routing.ps1`.
- Validate docs vocabulary and canonical links with `.github/hooks/validate-doc-contracts.ps1`.
- Keep catch-all matcher `(?s).+` as a single final route.
- Keep hook-routed agents documented in `AGENTS.md`, `.github/hooks/TRIGGERS.md`, and `docs/automation/agent-skill-invocation-playbook.md`.
