# Rust Lane Agent Guide (`crates/`)

Root baseline from [`../AGENTS.md`](../AGENTS.md) applies here. This file adds Rust-lane-specific
rules and may override root guidance for paths under `crates/`.

## Scope

Applies to `crates/**` and Rust workspace changes that primarily affect runtime, SDK, or crate
boundaries.

## Coding Standards

- `snake_case` for variables/functions/modules
- `PascalCase` for types/traits
- `SCREAMING_SNAKE_CASE` for constants
- Max line length: 100 characters
- Prefer `?` and explicit error types over `unwrap()` in library paths

## Error Handling and Unsafe Rules

- Avoid panics in recoverable library paths.
- Use contextual error variants/messages at boundaries.
- Every `unsafe` block must include a `// SAFETY:` comment.
- Keep unsafe code localized and wrap with safe APIs.

## Verification Gates (Rust Lane)

Run the minimal affected-scope checks first, then escalate to workspace checks when needed.

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

## Completion Checklist

1. Tests and lint/format gates pass for affected scope.
2. Public interfaces and contract docs are updated when changed.
3. Unsafe and error-handling policies remain satisfied.
4. `docs/crate-boundaries.md` is updated when placement responsibilities changed.
5. Changes are committed with a message that explains what changed and why.
