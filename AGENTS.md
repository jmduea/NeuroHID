# NeuroHID Agent Onboarding

This file defines project-wide baseline guidance for agents working in this repository.

## Scope and Precedence

- This root file applies to the entire repository.
- Additional `AGENTS.md` files inside subdirectories may add or explicitly override guidance for
  their subtree.
- Precedence rule: root baseline + nearest local `AGENTS.md` override/add.

### Python Command Policy

- Use `uv` for Python execution and tooling commands.
- Do not use bare `python` commands in docs, scripts, or automation.
- Prefer forms such as `uv run --project python ...` or `uv run --directory python ...`.

### Error Handling and Unsafe Guardrails

- Prefer recoverable errors (`Result`/`?`) over `unwrap()` in library paths.
- Every `unsafe` block must include a `// SAFETY:` rationale comment.
- Keep unsafe usage minimal and encapsulate it behind safe abstractions when possible.
