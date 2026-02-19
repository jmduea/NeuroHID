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
# Start bridge with canonical IPC defaults
uv run --directory python neurohid-ml bridge

# Start bridge with explicit canonical TCP endpoint
uv run --directory python neurohid-ml bridge --ipc-mode tcp_loopback --ipc-endpoint 127.0.0.1:47384
```

## Control and Telemetry Helpers

```bash
uv run --directory python neurohid-ml control snapshot --ipc-mode local_socket --ipc-endpoint neurohid.control.v3
uv run --directory python neurohid-ml telemetry-read --max-messages 1 --ipc-mode local_socket --ipc-endpoint neurohid.control.v3
```

Canonical public IPC arguments are `--ipc-mode` and `--ipc-endpoint`.
Legacy `--transport/--host/--port/--pipe-name` flags remain compatibility aliases and emit
deprecation warnings when used.

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
