# NeuroHID Hook Routing

This directory contains NeuroHID-specific hook routing.

- Shared cross-project Rust hook behavior remains in `.rust-skills/hooks/hooks.json`.
- Repo-specific workflow routing lives in `.github/hooks/hooks.json`.

## Event Contract

Current manifest defines `UserPromptSubmit` matchers and routes to `.github/agents/*` assets.

## Maintenance

- Keep matchers minimal and deterministic.
- Prefer updating agent logic over broadening regex matchers.
- Review false positives monthly.
