# Agent Instructions - NeuroHID

## Coding Standards

### Rust Development

- Always collapse if statements per <https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_if>
- Always inline format! args when possible per <https://rust-lang.github.io/rust-clippy/master/index.html#uninlined_format_args>
- Use method references over closures when possible per <https://rust-lang.github.io/rust-clippy/master/index.html#redundant_closure_for_method_calls>

### Python Development

- Use `uv` for Python execution and tooling commands.
- Do not use bare `python` commands in docs, scripts, or automation.
- Prefer free-threaded python versions 3.14t+
- Format with Ruff (`ruff format .`)
- Lint with Ruff (`ruff check .`)
- Test with pytest (`pytest --cov`)
- Always include type hints
- Use Google-style docstrings
- Follow PEP 8 with max line length 120

### Naming Conventions

- Functions/variables: snake_case
- Classes: PascalCase
- Constants: UPPER_SNAKE_CASE
- Files: snake_case.py

## Git Workflow

Use Conventional Commits:

- `feat:` new feature
- `fix:` bug fix
- `docs:` documentation changes
- `refactor:` code refactoring
- `test:` adding or updating tests
- `chore:` maintenance tasks

Always:

1. Run tests before committing
2. Write clear, concise commit messages

## Workflow Guidelines

### Planning

- Start complex tasks in Plan mode
- Get the plan right before implementing
- Break large tasks into smaller, focused steps

### Verification

- Always verify work with tests when available
- Run linter after making changes
- Test UI changes in browser when applicable

### Error Handling

- Use try-except with proper logging
- Provide clear error messages
- Don't silently ignore exceptions

## Key Files

| File | Purpose |
|------|---------|
| `CLAUDE.md` | Claude Code memory (project-specific) |
| `AGENTS.md` | This file - Cursor agent instructions |
| `.cursor/skills/` | Modular Cursor skills |

## Tools and Commands

| Task | Command |
|------|---------|
| Format code | `ruff format .` |
| Lint code | `ruff check .` |
| Fix lint issues | `ruff check --fix .` |
| Run tests | `pytest` |
| Run with coverage | `pytest --cov` |

## Preferences

- Provide concise, focused responses
- Show code examples when helpful
- Explain the "why" behind changes
- Prefer editing existing files over creating new ones
