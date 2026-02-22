# NeuroHID Agent Onboarding

This file defines project-wide baseline guidance for agents working in this repository.

## Scope and Precedence

- This root file applies to the entire repository.
- Additional `AGENTS.md` files inside subdirectories may add or explicitly override guidance for
  their subtree.
- Precedence rule: root baseline + nearest local `AGENTS.md` override/add.

## Repository Mission and Lanes

NeuroHID is a hybrid Rust/Python monorepo for translating EEG signals into standard HID actions.

Primary work lanes:

- `crates/`: Rust runtime, device/signal/action stack, IPC, GUI, SDK
- `python/`: ML bridge, decoder/ErrP/trainer flows, notebooks
- `docs/`: architecture, contracts, development/deployment/contribution guidance
- `.github/`: CI workflows and automation scripts

## Agent Startup Checklist

1. Identify the target lane (`crates/`, `python/`, `docs/`, `.github/`).
2. Read this root `AGENTS.md`.
3. Read the nearest lane-specific `AGENTS.md` (if present).
4. Follow lane-specific verification gates before completion.
5. Update affected documentation when behavior/contracts/workflows change.
6. Commit completed changes with a message that explains what changed and why.

## Global Baseline Policies

### Rust Project Defaults

When creating Rust projects or `Cargo.toml` package stanzas, use:

```toml
[package]
edition = "2024"
rust-version = "1.85"

[lints.rust]
unsafe_code = "warn"

[lints.clippy]
all = "warn"
pedantic = "warn"
```

### Python Command Policy

- Use `uv` for Python execution and tooling commands.
- Do not use bare `python` commands in docs, scripts, or automation.
- Prefer forms such as `uv run --project python ...` or `uv run --directory python ...`.

### Error Handling and Unsafe Guardrails

- Prefer recoverable errors (`Result`/`?`) over `unwrap()` in library paths.
- Every `unsafe` block must include a `// SAFETY:` rationale comment.
- Keep unsafe usage minimal and encapsulate it behind safe abstractions when possible.

### Git Safety

- Do not revert unrelated user changes.
- Avoid destructive commands unless explicitly requested.
- Prefer non-interactive git commands.
- Commit completed work unless the user explicitly asks not to commit.
- Use clear commit messages that summarize scope and intent, not vague messages.

## Lane-Specific Agent Guides

- Rust lane: [`crates/AGENTS.md`](./crates/AGENTS.md)
- Python lane: [`python/AGENTS.md`](./python/AGENTS.md)
- Documentation lane: [`docs/AGENTS.md`](./docs/AGENTS.md)

For architecture and operational context, use [`docs/index.md`](./docs/index.md).
