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
- Python version: `>=3.14` (free-threaded CPython)

## Test Surface

Python tests are present under `python/tests/` for bridge, decoder/ErrP, control client, trainer,
notebook helpers, and lab kernel behavior.

## Integration Boundary

This package communicates with the Rust runtime **in-process** via PyO3 bindings
(`neurohid-py` crate, module name `neurohid`). Python code receives samples,
features, actions, markers, and runtime events as async iterators and sends
commands/trainer messages through `RuntimeHandle` methods — no socket transport or
serialization overhead. See [ADR-001](adr/ADR-001-in-process-python-bindings.md).
