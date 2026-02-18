# neurohid-ml

Python ML tooling for NeuroHID, including:

- Runtime bridge (`neurohid-ml bridge`)
- Control and telemetry helpers
- Decoder and ErrP model workflows
- Trainer and candidate model staging commands
- Notebook/Jupyter workflows

## Setup

From repository root:

```bash
uv sync --directory python
uv run --directory python neurohid-ml --help
```

## Runtime Bridge Workflows

```bash
# Start bridge with default transport settings
uv run --directory python neurohid-ml bridge

# Start bridge with explicit TCP loopback transport
uv run --directory python neurohid-ml bridge --transport tcp_loopback --port 47384
```

## Control and Telemetry Helpers

```bash
uv run --directory python neurohid-ml control snapshot
uv run --directory python neurohid-ml telemetry-read --max-messages 1
```

## Training and Candidate Staging

```bash
# Train + stage candidate for a profile
uv run --directory python neurohid-ml train-profile-candidate --profile-id <PROFILE_ID>

# Continuous trainer worker loop
uv run --directory python neurohid-ml trainer-worker --profile-id <PROFILE_ID>
```

## Jupyter and Notebook Workflows

```bash
# Start JupyterLab from repository root
uv run --directory python jupyter lab
```

Notebooks are under `python/notebooks`.

If using the Hub's Advanced mode, Jupyter IDE and Python Lab views rely on the same
`python/` environment prepared via `uv sync --directory python`.

## Python Quality Gates

```bash
uv run --project python pytest python/tests -q
uv run --project python ruff check python/src python/tests
uv run --project python black --check python/src python/tests
uv run --project python mypy python/src
```
