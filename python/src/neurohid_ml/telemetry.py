"""Notebook-facing telemetry client for Runtime-ML IPC v2."""

from __future__ import annotations

import json
import os
import socket
import struct
from dataclasses import dataclass
from typing import Any, BinaryIO, Iterator

from neurohid_ml.control import NotebookError

DEFAULT_ML_HOST = "127.0.0.1"
DEFAULT_ML_PORT = 47_384
DEFAULT_ML_PIPE_NAME = r"\\.\pipe\neurohid.ml.v2"


@dataclass(slots=True)
class NeuroHidTelemetryClient:
    """Synchronous telemetry envelope reader for notebooks."""

    transport: str = "named_pipe" if os.name == "nt" else "tcp_loopback"
    host: str = DEFAULT_ML_HOST
    port: int = DEFAULT_ML_PORT
    pipe_name: str = DEFAULT_ML_PIPE_NAME
    connect_timeout_secs: float = 1.5
    read_timeout_secs: float = 0.2

    def endpoint_label(self) -> str:
        mode = self.transport.strip().lower()
        if mode == "named_pipe":
            return self.pipe_name
        return f"{self.host}:{self.port}"

    def recv(self) -> dict[str, Any] | None:
        mode = self.transport.strip().lower()

        try:
            if mode == "named_pipe":
                if os.name != "nt":
                    raise NotebookError("transport='named_pipe' is only supported on Windows")
                with open(self.pipe_name, "r+b", buffering=0) as pipe:
                    return _read_framed_json(pipe)

            with socket.create_connection(
                (self.host, self.port),
                timeout=self.connect_timeout_secs,
            ) as conn:
                conn.settimeout(self.read_timeout_secs)
                return _read_framed_json(conn)
        except TimeoutError:
            return None
        except OSError as error:
            raise NotebookError(
                "unable to reach NeuroHID telemetry endpoint at "
                f"{self.endpoint_label()} ({error})"
            ) from error

    def iter_messages(self, *, max_messages: int | None = None) -> Iterator[dict[str, Any]]:
        emitted = 0
        while max_messages is None or emitted < max_messages:
            message = self.recv()
            if message is None:
                continue
            yield message
            emitted += 1


def _read_exact(reader: BinaryIO | socket.socket, size: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < size:
        if hasattr(reader, "recv"):
            chunk = reader.recv(size - len(chunks))
        else:
            chunk = reader.read(size - len(chunks))
        if not chunk:
            raise EOFError("telemetry endpoint closed")
        chunks.extend(chunk)
    return bytes(chunks)


def _read_framed_json(reader: BinaryIO | socket.socket) -> dict[str, Any] | None:
    length_bytes = _read_exact(reader, 4)
    frame_len = struct.unpack("<I", length_bytes)[0]
    if frame_len <= 0:
        return None

    payload = _read_exact(reader, frame_len)
    decoded = json.loads(payload.decode("utf-8"))
    if isinstance(decoded, dict):
        return decoded
    return None
