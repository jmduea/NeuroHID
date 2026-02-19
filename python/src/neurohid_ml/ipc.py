"""Unified IPC v3 transport helpers for NeuroHID Python clients."""

from __future__ import annotations

import json
import socket
import time
from dataclasses import dataclass
from typing import Any, Iterator, Sequence

from neurohid_ml.ipc_constants import (
    CANONICAL_IPC_MODE,
    CANONICAL_LOCAL_ENDPOINT,
    CANONICAL_TCP_HOST,
    CANONICAL_TCP_PORT,
    parse_tcp_endpoint as _parse_tcp_endpoint,
)

ipckit: Any
try:
    import ipckit  # type: ignore[import-not-found]
except Exception:  # pragma: no cover - optional dependency at import time
    ipckit = None


IPC_PROTOCOL_V3 = 3


def _read_exact(reader: Any, size: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < size:
        remaining = size - len(chunks)
        if hasattr(reader, "recv"):
            chunk = reader.recv(remaining)
        else:
            chunk = reader.read(remaining)
        if not chunk:
            raise OSError("unexpected EOF while reading framed IPC message")
        chunks.extend(chunk)
    return bytes(chunks)


def _read_framed_json(reader: Any) -> dict[str, Any]:
    len_buf = _read_exact(reader, 4)
    frame_len = int.from_bytes(len_buf, byteorder="little", signed=False)
    if frame_len <= 0:
        raise OSError("invalid zero-length IPC frame")
    payload = _read_exact(reader, frame_len)
    decoded = json.loads(payload.decode("utf-8", errors="replace"))
    if not isinstance(decoded, dict):
        raise OSError("invalid IPC payload: expected object")
    return decoded


def _encode_framed_json(payload: dict[str, Any]) -> bytes:
    body = json.dumps(payload).encode("utf-8")
    return len(body).to_bytes(4, byteorder="little", signed=False) + body


def _runtime_event_payload(envelope: dict[str, Any]) -> dict[str, Any]:
    payload = envelope.get("payload")
    if not isinstance(payload, dict):
        raise RuntimeError(f"invalid runtime.events envelope payload: {envelope}")
    seq = envelope.get("seq")
    if isinstance(seq, int) and "_seq" not in payload:
        payload = dict(payload)
        payload["_seq"] = seq
    return payload


def observation_to_vector(event_payload: dict[str, Any]) -> list[float]:
    """Flatten one `observation_frame` payload into a numeric vector."""
    observation = event_payload.get("observation")
    if not isinstance(observation, dict):
        raise RuntimeError("event payload does not include `observation` object")

    vector: list[float] = []
    signal = observation.get("signal_features")
    if isinstance(signal, dict):
        values = signal.get("values")
        if isinstance(values, list):
            vector.extend(float(v) for v in values)

    cursor = observation.get("cursor")
    if isinstance(cursor, dict):
        vector.extend(
            [
                float(cursor.get("x", 0.0)),
                float(cursor.get("y", 0.0)),
                float(cursor.get("velocity_x", 0.0)),
                float(cursor.get("velocity_y", 0.0)),
                1.0 if bool(cursor.get("button_held", False)) else 0.0,
            ]
        )

    screen = observation.get("screen")
    if isinstance(screen, dict):
        width = float(screen.get("width", 1.0) or 1.0)
        height = float(screen.get("height", 1.0) or 1.0)
        monitors = float(screen.get("monitor_count", 1.0) or 1.0)
        aspect = width / max(height, 1.0)
        vector.extend([aspect, monitors])

    return vector


def observation_to_numpy(event_payload: dict[str, Any]) -> Any:
    """Return NumPy array view of one observation event payload."""
    try:
        import numpy as np
    except Exception as error:  # pragma: no cover - optional dependency
        raise RuntimeError("NumPy is required for observation_to_numpy()") from error
    return np.asarray(observation_to_vector(event_payload), dtype=np.float32)


def events_to_dataframe(events: Sequence[dict[str, Any]]) -> Any:
    """Convert a sequence of runtime event payloads to a pandas DataFrame."""
    try:
        import pandas as pd  # type: ignore[import-untyped]
    except Exception as error:  # pragma: no cover - optional dependency
        raise RuntimeError("pandas is required for events_to_dataframe()") from error

    rows: list[dict[str, Any]] = []
    for payload in events:
        if not isinstance(payload, dict):
            continue
        row = {"event_type": payload.get("type")}
        if payload.get("type") == "observation_frame":
            row["observation_vector"] = observation_to_vector(payload)
        observation = payload.get("observation")
        if isinstance(observation, dict):
            row["timestamp"] = observation.get("timestamp")
            signal = observation.get("signal_features")
            if isinstance(signal, dict):
                values = signal.get("values")
                if isinstance(values, list):
                    row["feature_dim"] = len(values)
        rows.append(row)
    return pd.DataFrame(rows)


@dataclass(slots=True)
class NeuroHidIpcClient:
    """Unified IPC v3 client supporting local-socket and TCP loopback modes."""

    ipc_mode: str = CANONICAL_IPC_MODE
    ipc_endpoint: str = CANONICAL_LOCAL_ENDPOINT
    host: str = CANONICAL_TCP_HOST
    port: int = CANONICAL_TCP_PORT
    connect_timeout_secs: float = 1.5
    read_timeout_secs: float = 1.5
    connect_retries: int = 1

    def endpoint_label(self) -> str:
        mode = self.ipc_mode.strip().lower()
        if mode == "local_socket":
            return self.ipc_endpoint
        endpoint = self.ipc_endpoint.strip()
        if ":" in endpoint:
            return endpoint
        return f"{self.host}:{self.port}"

    def _tcp_target(self) -> tuple[str, int]:
        endpoint = self.ipc_endpoint.strip()
        if ":" in endpoint:
            return _parse_tcp_endpoint(endpoint)

        # Compatibility alias path while callers migrate to canonical ipc_endpoint.
        host = self.host.strip() or "127.0.0.1"
        port = int(self.port)
        if port <= 0 or port > 65_535:
            raise RuntimeError(f"invalid tcp_loopback port value: {self.port}")
        return host, port

    def _base_envelope(
        self,
        *,
        channel: str,
        msg_type: str,
        session_id: str,
        payload: dict[str, Any],
        request_id: str | None = None,
    ) -> dict[str, Any]:
        return {
            "v": IPC_PROTOCOL_V3,
            "channel": channel,
            "msg_type": msg_type,
            "seq": 1,
            "request_id": request_id,
            "session_id": session_id,
            "sent_at_us": int(time.time() * 1_000_000),
            "payload": payload,
        }

    def send_envelope(self, envelope: dict[str, Any]) -> dict[str, Any]:
        mode = self.ipc_mode.strip().lower()
        if mode == "local_socket":
            if ipckit is None:
                raise RuntimeError("ipckit package is required for local_socket IPC mode")
            channel = ipckit.IpcChannel.connect(self.ipc_endpoint)
            channel.send_json(envelope)
            response = channel.recv_json()
            if not isinstance(response, dict):
                raise RuntimeError("invalid local_socket IPC response: expected object")
            return response

        retries = max(self.connect_retries, 0)
        attempt = 0
        last_error: Exception | None = None
        frame = _encode_framed_json(envelope)
        host, port = self._tcp_target()
        while attempt <= retries:
            try:
                with socket.create_connection(
                    (host, port),
                    timeout=self.connect_timeout_secs,
                ) as conn:
                    conn.sendall(frame)
                    conn.settimeout(self.read_timeout_secs)
                    return _read_framed_json(conn)
            except (TimeoutError, OSError) as error:
                last_error = error
                attempt += 1
                if attempt <= retries:
                    time.sleep(0.15)

        assert last_error is not None
        raise last_error

    def send_control_command(self, command: dict[str, Any]) -> dict[str, Any]:
        envelope = self._base_envelope(
            channel="control.rpc",
            msg_type="request",
            session_id="python-control",
            payload={
                "request_id": None,
                "command": command,
            },
        )
        response = self.send_envelope(envelope)
        payload = response.get("payload")
        if not isinstance(payload, dict):
            raise RuntimeError(f"invalid control envelope payload: {response}")
        return payload

    def poll_runtime_event(self, *, family: str | None = None) -> dict[str, Any]:
        payload: dict[str, Any] = {}
        if family is not None:
            payload["family"] = family
        envelope = self._base_envelope(
            channel="runtime.events",
            msg_type="poll",
            session_id="python-events",
            payload=payload,
        )
        response = self.send_envelope(envelope)
        return _runtime_event_payload(response)

    def iter_runtime_events(
        self,
        *,
        max_messages: int | None = None,
        families: Sequence[str] | None = None,
        resume_from_seq: int | None = None,
        sample_every: int = 1,
        max_duration_ms: int | None = None,
        snapshot_interval_ms: int = 1_000,
        prefer_stream: bool = True,
    ) -> Iterator[dict[str, Any]]:
        if prefer_stream:
            try:
                yield from self._iter_runtime_events_subscribe(
                    max_messages=max_messages,
                    families=families,
                    resume_from_seq=resume_from_seq,
                    sample_every=sample_every,
                    max_duration_ms=max_duration_ms,
                    snapshot_interval_ms=snapshot_interval_ms,
                )
                return
            except (RuntimeError, TimeoutError, OSError):
                # Fallback keeps notebooks functional on older services.
                pass

        requested = [f for f in (families or []) if isinstance(f, str) and f.strip()]
        if not requested:
            requested = ["snapshot"]
        emitted = 0
        index = 0
        while max_messages is None or emitted < max_messages:
            family = requested[index % len(requested)]
            index += 1
            yield self.poll_runtime_event(family=family)
            emitted += 1

    def _iter_runtime_events_subscribe(
        self,
        *,
        max_messages: int | None,
        families: Sequence[str] | None,
        resume_from_seq: int | None,
        sample_every: int,
        max_duration_ms: int | None,
        snapshot_interval_ms: int,
    ) -> Iterator[dict[str, Any]]:
        subscription_payload: dict[str, Any] = {
            "families": [f for f in (families or []) if isinstance(f, str) and f.strip()],
            "include_snapshot": True,
            "include_capabilities": True,
            "sample_every": max(int(sample_every), 1),
            "snapshot_interval_ms": max(int(snapshot_interval_ms), 100),
        }
        if max_messages is not None:
            subscription_payload["max_events"] = max(int(max_messages), 1)
        if max_duration_ms is not None:
            subscription_payload["max_duration_ms"] = max(int(max_duration_ms), 1)
        if resume_from_seq is not None:
            subscription_payload["resume_from_seq"] = max(int(resume_from_seq), 0)

        envelope = self._base_envelope(
            channel="runtime.events",
            msg_type="subscribe",
            session_id="python-events",
            payload=subscription_payload,
        )

        mode = self.ipc_mode.strip().lower()
        emitted = 0

        if mode == "local_socket":
            if ipckit is None:
                raise RuntimeError("ipckit package is required for local_socket IPC mode")
            channel = ipckit.IpcChannel.connect(self.ipc_endpoint)
            channel.send_json(envelope)
            while max_messages is None or emitted < max_messages:
                response = channel.recv_json()
                if not isinstance(response, dict):
                    raise RuntimeError("invalid runtime.events stream envelope")
                payload = _runtime_event_payload(response)
                yield payload
                emitted += 1
                if (
                    payload.get("type") == "lifecycle"
                    and payload.get("state") == "subscription_closed"
                ):
                    break
            return

        host, port = self._tcp_target()
        with socket.create_connection(
            (host, port),
            timeout=self.connect_timeout_secs,
        ) as conn:
            conn.settimeout(self.read_timeout_secs)
            conn.sendall(_encode_framed_json(envelope))
            while max_messages is None or emitted < max_messages:
                response = _read_framed_json(conn)
                payload = _runtime_event_payload(response)
                yield payload
                emitted += 1
                if (
                    payload.get("type") == "lifecycle"
                    and payload.get("state") == "subscription_closed"
                ):
                    break
