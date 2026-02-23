# Python Lane Agent Guide (`python/`)

Root baseline from [`../AGENTS.md`](../AGENTS.md) applies here. This file adds Python-lane-specific
rules and may override root guidance for paths under `python/`.

## Scope

Applies to `python/**`, including `src/neurohid_ml`, tests, and notebooks.

## Command Policy (`uv`-first)

Use `uv` for all Python commands.

```bash
uv sync --directory python
uv run --directory python neurohid-ml --help
uv run --directory python neurohid-ml bridge
uv run --directory python neurohid-ml train-profile-candidate --profile-id <PROFILE_ID>
uv run --directory python neurohid-ml trainer-worker --profile-id <PROFILE_ID>
```

Avoid bare `python` commands in docs, scripts, and automation.

## Quality Gate Sequence

Run in this order unless a narrower scope is sufficient:

```bash
uv run --project python pytest python/tests -q
uv run --project python ruff check python/src python/tests
uv run --project python black --check python/src python/tests
uv run --project python mypy python/src
```

## Completion Checklist

1. Updated/added tests for behavior changes.
2. Pytest + lint/format/type checks pass for affected scope.
3. Changes are committed with a message that explains what changed and why.
