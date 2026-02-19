"""Canonical IPC endpoint defaults shared across Python NeuroHID clients."""

from __future__ import annotations

CANONICAL_IPC_MODE = "local_socket"
CANONICAL_TCP_HOST = "127.0.0.1"
CANONICAL_TCP_PORT = 47_384
CANONICAL_LOCAL_ENDPOINT = "neurohid.control.v3"
CANONICAL_TCP_ENDPOINT = f"{CANONICAL_TCP_HOST}:{CANONICAL_TCP_PORT}"

# Compatibility aliases retained while callers migrate.
DEFAULT_CONTROL_PIPE_NAME = r"\\.\pipe\neurohid.control.v3"
DEFAULT_ML_PIPE_NAME = r"\\.\pipe\neurohid.ml.v3"
