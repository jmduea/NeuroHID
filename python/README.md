# neurohid-ml

Python ML tooling for NeuroHID, including:

- Runtime bridge (`neurohid-ml bridge`)
- Control and telemetry helpers
- Notebook helper APIs
- Candidate model training/staging commands

## Quickstart

From repository root:

```bash
uv sync --directory python
uv run --directory python neurohid-ml --help
```

## Common Commands

```bash
uv run --directory python neurohid-ml bridge
uv run --directory python neurohid-ml control snapshot
uv run --directory python neurohid-ml telemetry-read --max-messages 1
uv run --directory python neurohid-ml train-profile-candidate --profile-id <PROFILE_ID>
```
