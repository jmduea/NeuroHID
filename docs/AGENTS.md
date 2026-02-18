# Documentation Lane Agent Guide (`docs/`)

Root baseline from [`../AGENTS.md`](../AGENTS.md) applies here. This file adds documentation-lane
rules and may override root guidance for paths under `docs/`.

## Scope

Applies to `docs/**` and documentation architecture decisions across the repository.

## Information Architecture Policy

Use the following canonical placement rules:

- `README.md`: product introduction and high-level purpose only
- `docs/development-guide.md`: setup, build, test, automation, CI gate workflows
- `docs/deployment-guide.md`: runtime operation, transports, control endpoint, observability,
  validation harness
- `python/README.md`: Python package commands and ML workflow operations
- `CONTRIBUTING.md`: contributor process and PR policy
- `docs/index.md`: authoritative documentation map

Do not duplicate large policy/command blocks across multiple docs when one canonical home exists.

## README Scope Guardrail

When editing root `README.md`:

- Keep focus on "what NeuroHID is" and high-level architecture/value.
- Do not add operational runbooks, CI admin procedures, automation scripts, or phased project plans.
- Link to focused docs instead of embedding full workflows.

## Link Hygiene

- Prefer repository-relative Markdown links for local docs.
- Validate that every referenced path exists.
- Remove or replace stale references instead of leaving placeholders.

Do not leave references to non-existent documentation subtrees.

## Documentation Update Triggers

Update docs when any of the following change:

- Public/runtime/control/protocol contracts
- Build/test/run workflows
- CI gate names or branch/release policies
- Directory ownership and crate/package boundaries

## Completion Checklist

1. Changed docs have one clear purpose each.
2. Canonical-home routing is respected.
3. Links resolve to existing targets.
4. `docs/index.md` reflects new/relocated docs.
5. Changes are committed with a message that explains what changed and why.
