# Architecture: Python ML (`python-ml`)

## Scope

This document covers the Python package in `python/` (`src/neurohid_ml`) and related tests/notebooks.

## Architectural Style

- Package-based ML module with CLI entrypoint
- Separation of concerns across bridge, decoding, ErrP detection, and training components
- Notebook-compatible workflows for experimentation and observability

## Package Structure

| Area | Purpose |
|---|---|
| `bridge/` | Runtime IPC bridge client behavior |
| `decoder/` | Policy inference and model interaction |
| `errp/` | Error-related potential classifier logic |
| `trainer/` | Training loop and model candidate workflows |
| `cli.py` | Operational command-line interface |
| `notebook.py` | Jupyter and notebook helper integration |

## Dependencies and Tooling

- Runtime: `torch`, `onnx`, `numpy`, `scipy`, `scikit-learn`, `jupyterlab`
- Dev quality: `pytest`, `pytest-cov`, `pytest-asyncio`, `black`, `ruff`, `mypy`
- Python version: `>=3.12`

## Test Surface

Python tests are present under `python/tests/` for bridge, decoder/ErrP, control client, trainer,
notebook helpers, and lab kernel behavior.

## Integration Boundary

This part receives runtime events and returns ML outputs/health through the local bridge protocol,
keeping model lifecycle concerns isolated from latency-critical Rust components.
