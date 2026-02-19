"""Notebook-facing runtime-events client for IPC v3."""

from __future__ import annotations

import json
import socket
import struct
import warnings
from dataclasses import dataclass
from typing import Any, BinaryIO, Iterator

from neurohid_ml.control import NotebookError
from neurohid_ml.ipc import NeuroHidIpcClient
from neurohid_ml.ipc_constants import (
    CANONICAL_IPC_MODE,
    CANONICAL_LOCAL_ENDPOINT,
    CANONICAL_TCP_HOST,
    CANONICAL_TCP_PORT,
    DEFAULT_CONTROL_PIPE_NAME,
)


@dataclass(slots=True)
class NeuroHidTelemetryClient:
    """Synchronous runtime.events reader for notebooks."""

    ipc_mode: str = CANONICAL_IPC_MODE
    ipc_endpoint: str = CANONICAL_LOCAL_ENDPOINT
    transport: str | None = None  # Legacy alias.
    host: str = CANONICAL_TCP_HOST
    port: int = CANONICAL_TCP_PORT
    pipe_name: str = DEFAULT_CONTROL_PIPE_NAME  # Legacy alias.
    connect_timeout_secs: float = 1.5
    read_timeout_secs: float = 0.5
    connect_retries: int = 1

    def endpoint_label(self) -> str:
        return self._ipc().endpoint_label()

    def recv(self) -> dict[str, Any] | None:
        try:
            event = self._ipc().poll_runtime_event(family="runtime_telemetry")
            return event
        except TimeoutError:
            return None
        except OSError as error:
            raise NotebookError(
                "unable to reach NeuroHID runtime.events endpoint at "
                f"{self.endpoint_label()} ({error})"
            ) from error
        except RuntimeError as error:
            raise NotebookError(str(error)) from error

    def iter_messages(
        self,
        *,
        max_messages: int | None = None,
        families: list[str] | None = None,
        resume_from_seq: int | None = None,
        sample_every: int = 1,
        max_duration_ms: int | None = None,
        snapshot_interval_ms: int = 1_000,
        prefer_stream: bool = True,
    ) -> Iterator[dict[str, Any]]:
        requested_families = families if families else ["runtime_telemetry"]
        yield from self._ipc().iter_runtime_events(
            max_messages=max_messages,
            families=requested_families,
            resume_from_seq=resume_from_seq,
            sample_every=sample_every,
            max_duration_ms=max_duration_ms,
            snapshot_interval_ms=snapshot_interval_ms,
            prefer_stream=prefer_stream,
        )

    def _ipc(self) -> NeuroHidIpcClient:
        mode = self.ipc_mode.strip().lower()
        legacy_alias_used = False
        if not mode and self.transport:
            legacy_mode = self.transport.strip().lower()
            if legacy_mode == "named_pipe":
                mode = "local_socket"
            elif legacy_mode in {"tcp", "tcp_loopback"}:
                mode = "tcp_loopback"
            legacy_alias_used = True
        if not mode:
            mode = "local_socket"
        endpoint = self.ipc_endpoint
        if mode == "local_socket" and self.pipe_name and self.pipe_name != self.ipc_endpoint:
            endpoint = self.pipe_name
            legacy_alias_used = True

        if (
            (self.transport or "").strip().lower() not in {"", "tcp_loopback"}
            or self.host.strip() != CANONICAL_TCP_HOST
            or int(self.port) != CANONICAL_TCP_PORT
            or self.pipe_name != DEFAULT_CONTROL_PIPE_NAME
        ):
            legacy_alias_used = True

        if legacy_alias_used:
            warnings.warn(
                "transport/host/port/pipe_name are deprecated aliases; "
                "prefer ipc_mode/ipc_endpoint",
                DeprecationWarning,
                stacklevel=2,
            )

        return NeuroHidIpcClient(
            ipc_mode=mode,
            ipc_endpoint=endpoint,
            host=self.host,
            port=self.port,
            connect_timeout_secs=self.connect_timeout_secs,
            read_timeout_secs=self.read_timeout_secs,
            connect_retries=self.connect_retries,
        )


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
