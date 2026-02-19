# NeuroHID Starter Notebooks

These notebooks are designed for the managed Jupyter IDE flow in Hub Advanced mode.

- `00_scratchpad.ipynb`: quick sandbox and connectivity check.
- `10_live_runtime_monitor.ipynb`: inspect runtime/bridge status and issue control commands.
- `20_offline_training_flow.ipynb`: train/stage candidate artifacts from profile data.
- `30_eegnet_from_runtime_features.ipynb`: set up PyTorch (GPU if available), collect runtime
    feature vectors, and train an EEGNet-style classifier.

## Preconditions

- NeuroHID service is running.
- Canonical IPC endpoint is available (`ipc_mode=local_socket`, `ipc_endpoint=neurohid.control.v3`).
- Python environment is bootstrapped via Hub (`Prepare Environment`).

## Programmatic Control Helpers

`NeuroHidNotebook` now exposes both control and telemetry convenience APIs:

- Control commands: `snapshot`, `trainer_snapshot`, `set_output_enabled`, `set_learning_enabled`,
  `set_fallback_policy`, `reload_model`, `promote_candidate_model`, stream connect/disconnect
  helpers.
- Telemetry reads: `recv_telemetry()` for one envelope and `iter_telemetry()` for continuous reads.

Defaults target:

- Canonical local socket endpoint: `neurohid.control.v3`.
- Canonical TCP fallback endpoint: `127.0.0.1:47384`.

## CLI Quickstart (uv)

From repository root:

- Snapshot runtime state: `uv run --project python neurohid-ml control snapshot`
- Toggle runtime output: `uv run --project python neurohid-ml control set_output_enabled --enabled false`
- Force bridge reconnect: `uv run --project python neurohid-ml control ml_bridge_reconnect`
- Ensure one EEG stream is connected: `uv run --project python neurohid-ml control ensure_connected_stream`
- Read one telemetry envelope: `uv run --project python neurohid-ml telemetry-read --max-messages 1`
- Tail 10 telemetry envelopes (TCP mode): `uv run --project python neurohid-ml telemetry-read --ipc-mode tcp_loopback --ipc-endpoint 127.0.0.1:47384 --max-messages 10`
