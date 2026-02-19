# Python Lane Agent Guide (`python/`)

Root baseline from [`../AGENTS.md`](../AGENTS.md) applies here. This file adds Python-lane-specific
rules and may override root guidance for paths under `python/`.

## Scope

Applies to `python/**`, including `src/neurohid_ml`, tests, and notebooks.

## Package Layout

- `src/neurohid_ml/bridge/`: runtime bridge client behavior
- `src/neurohid_ml/decoder/`: decoder/model inference logic
- `src/neurohid_ml/errp/`: error-related potential components
- `src/neurohid_ml/trainer/`: trainer loops and candidate staging
- `src/neurohid_ml/cli.py`: operational CLI entrypoint
- `tests/`: Python test surface
- `notebooks/`: exploratory notebooks

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

## Bridge and Trainer Workflow References

- Canonical Python command reference: [`README.md`](./README.md)
- Runtime and operations integration context:
  [`../docs/deployment-guide.md`](../docs/deployment-guide.md)
- Protocol contract reference:
  [`../docs/protocol-and-api.md`](../docs/protocol-and-api.md)

## Completion Checklist

1. Updated/added tests for behavior changes.
2. Pytest + lint/format/type checks pass for affected scope.
3. CLI/help text and docs updated when commands or flows change.
4. Runtime protocol assumptions stay aligned with `docs/protocol-and-api.md`.
5. Changes are committed with a message that explains what changed and why.
