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


def parse_tcp_endpoint(endpoint: str) -> tuple[str, int]:
    """Parse a ``host:port`` string into (host, port).

    Returns ``(CANONICAL_TCP_HOST, port)`` when the host portion is empty or
    whitespace-only.

    Raises ``RuntimeError`` for malformed or out-of-range values.
    """
    value = endpoint.strip()
    if not value:
        raise RuntimeError("ipc_endpoint must not be empty for tcp_loopback mode")
    host, sep, port_raw = value.rpartition(":")
    if sep == "":
        raise RuntimeError(f"invalid tcp_loopback ipc_endpoint '{endpoint}': expected host:port")
    host = host.strip() or CANONICAL_TCP_HOST
    if host.startswith("[") and host.endswith("]"):
        host = host[1:-1].strip() or CANONICAL_TCP_HOST
    try:
        port = int(port_raw)
    except ValueError as error:
        raise RuntimeError(
            f"invalid tcp_loopback ipc_endpoint '{endpoint}': invalid port '{port_raw}'"
        ) from error
    if port <= 0 or port > 65_535:
        raise RuntimeError(
            f"invalid tcp_loopback ipc_endpoint '{endpoint}': port {port} out of range"
        )
    return host, port
