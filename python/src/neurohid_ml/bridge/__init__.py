"""Runtime ML bridge (protocol v3) for NeuroHID.

This module implements the trainer-side endpoint for the NeuroHID runtime ML
protocol v3. On Windows it defaults to named pipes; on non-Windows it defaults
to TCP loopback for development.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import os
import struct
import time
import warnings
from dataclasses import dataclass
from enum import Enum
from typing import Any, BinaryIO, Optional

import numpy as np

from neurohid_ml.errp import ErrPConfig, ErrPDetector
from neurohid_ml.ipc_constants import (
    CANONICAL_IPC_MODE,
    CANONICAL_LOCAL_ENDPOINT,
    CANONICAL_TCP_HOST,
    CANONICAL_TCP_PORT,
    DEFAULT_ML_PIPE_NAME,
)

ipckit: Any
try:
    import ipckit  # type: ignore[import-not-found]
except Exception:  # pragma: no cover - optional dependency
    ipckit = None

IPC_PROTOCOL_V3 = 3
DEFAULT_IPC_PORT = CANONICAL_TCP_PORT
DEFAULT_IPC_ENDPOINT = CANONICAL_LOCAL_ENDPOINT
DEFAULT_PIPE_NAME = DEFAULT_ML_PIPE_NAME
DEFAULT_HOST = CANONICAL_TCP_HOST


class IpcTransport(str, Enum):
    """Transport mode used by the trainer bridge."""

    NAMED_PIPE = "named_pipe"
    TCP_LOOPBACK = "tcp_loopback"


@dataclass
class IpcConfig:
    """Configuration for trainer<->runtime ML bridge connectivity."""

    ipc_mode: str | None = CANONICAL_IPC_MODE
    ipc_endpoint: str = DEFAULT_IPC_ENDPOINT
    transport: IpcTransport | str = (
        IpcTransport.NAMED_PIPE if os.name == "nt" else IpcTransport.TCP_LOOPBACK
    )  # Legacy alias.
    host: str = DEFAULT_HOST
    port: int = DEFAULT_IPC_PORT
    pipe_name: str = DEFAULT_PIPE_NAME  # Legacy alias.
    connect_timeout_sec: float = 5.0
    recv_timeout_sec: float = 0.2
    auto_reconnect: bool = True
    reconnect_delay_sec: float = 1.0
    max_reconnect_attempts: int = 0
    heartbeat_interval_sec: float = 0.5

    def __post_init__(self) -> None:
        mode = (self.ipc_mode or "").strip().lower()
        transport_raw = (
            self.transport.value
            if isinstance(self.transport, IpcTransport)
            else str(self.transport).strip().lower()
        )
        if not mode:
            if transport_raw in {"named_pipe", "pipe", "local_socket"}:
                mode = "local_socket"
            elif transport_raw in {"tcp", "tcp_loopback"}:
                mode = "tcp_loopback"
            else:
                mode = "local_socket"
        if mode not in {"local_socket", "tcp_loopback"}:
            raise RuntimeError(f"unsupported ipc_mode '{self.ipc_mode}'")

        endpoint = self.ipc_endpoint.strip()
        if not endpoint:
            if mode == "tcp_loopback":
                endpoint = f"{self.host}:{self.port}"
            else:
                endpoint = self.pipe_name or DEFAULT_IPC_ENDPOINT

        if mode == "tcp_loopback":
            if ":" not in endpoint:
                warnings.warn(
                    "bridge host/port aliases are deprecated; prefer ipc_endpoint='host:port'",
                    DeprecationWarning,
                    stacklevel=2,
                )
                endpoint = f"{self.host}:{self.port}"
            host, port = _parse_tcp_endpoint(endpoint)
            self.host = host
            self.port = port
            self.transport = IpcTransport.TCP_LOOPBACK
        else:
            if endpoint == DEFAULT_IPC_ENDPOINT and self.pipe_name != DEFAULT_PIPE_NAME:
                warnings.warn(
                    "bridge pipe_name alias is deprecated; prefer ipc_endpoint",
                    DeprecationWarning,
                    stacklevel=2,
                )
                endpoint = self.pipe_name
            self.pipe_name = endpoint
            self.transport = IpcTransport.NAMED_PIPE

        self.ipc_mode = mode
        self.ipc_endpoint = endpoint

        if isinstance(self.transport, str):
            self.transport = IpcTransport(self.transport)


def _parse_tcp_endpoint(endpoint: str) -> tuple[str, int]:
    value = endpoint.strip()
    if not value:
        raise RuntimeError("ipc_endpoint must not be empty for tcp_loopback mode")
    host, sep, port_raw = value.rpartition(":")
    if sep == "":
        raise RuntimeError(f"invalid tcp_loopback ipc_endpoint '{endpoint}': expected host:port")
    host = host.strip() or DEFAULT_HOST
    if host.startswith("[") and host.endswith("]"):
        host = host[1:-1].strip() or DEFAULT_HOST
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


@dataclass
class BridgeStats:
    """Lightweight trainer runtime stats surfaced via `trainer_status`."""

    replay_size: int = 0
    training_step: int = 0
    policy_loss: Optional[float] = None
    value_loss: Optional[float] = None
    entropy: Optional[float] = None
    last_error: Optional[str] = None
    decoder_confidence_ema: Optional[float] = None
    signal_quality_ema: Optional[float] = None


def now_micros() -> int:
    """Current unix timestamp in microseconds."""

    return time.time_ns() // 1_000


def _quality_label(score: float) -> str:
    if score >= 0.75:
        return "good"
    if score >= 0.5:
        return "acceptable"
    if score >= 0.25:
        return "poor"
    return "unusable"


def _ema(previous: Optional[float], value: float, alpha: float = 0.1) -> float:
    value = float(value)
    if previous is None:
        return value
    return float(previous * (1.0 - alpha) + value * alpha)


def _bernoulli_entropy(probability: float) -> float:
    p = float(np.clip(probability, 1e-6, 1.0 - 1e-6))
    return float(-(p * np.log(p) + (1.0 - p) * np.log(1.0 - p)))


class IpcClient:
    """Trainer-side IPC client with v3 envelope framing."""

    def __init__(self, config: IpcConfig):
        self.config = config
        self.reader: Optional[asyncio.StreamReader] = None
        self.writer: Optional[asyncio.StreamWriter] = None
        self.pipe: Optional[BinaryIO] = None
        self.ipc_channel: Any | None = None
        self.connected = False
        self.sequence = 0

    async def connect(self) -> bool:
        """Connect to runtime bridge endpoint."""

        try:
            if self.config.ipc_mode == "tcp_loopback":
                reader, writer = await asyncio.wait_for(
                    asyncio.open_connection(self.config.host, self.config.port),
                    timeout=self.config.connect_timeout_sec,
                )
                self.reader = reader
                self.writer = writer

                sock = writer.get_extra_info("socket")
                if sock is not None:
                    import socket

                    sock.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            else:
                if ipckit is not None:
                    self.ipc_channel = await asyncio.wait_for(
                        asyncio.to_thread(ipckit.IpcChannel.connect, self.config.ipc_endpoint),
                        timeout=self.config.connect_timeout_sec,
                    )
                else:
                    if os.name != "nt":
                        raise RuntimeError(
                            "named_pipe transport is only supported on Windows hosts"
                        )
                    self.pipe = await asyncio.wait_for(
                        asyncio.to_thread(self._open_named_pipe_blocking),
                        timeout=self.config.connect_timeout_sec,
                    )

            self.connected = True
            return True
        except Exception as error:  # noqa: BLE001
            endpoint = self.endpoint_label()
            print(f"Bridge connect failed ({endpoint}): {error}")
            self.connected = False
            self.reader = None
            self.writer = None
            self.pipe = None
            self.ipc_channel = None
            return False

    async def disconnect(self) -> None:
        """Disconnect and release transport handles."""

        if self.writer is not None:
            self.writer.close()
            await self.writer.wait_closed()
        self.writer = None
        self.reader = None

        if self.ipc_channel is not None:
            channel = self.ipc_channel
            self.ipc_channel = None
            close_fn = getattr(channel, "close", None)
            if callable(close_fn):
                await asyncio.to_thread(close_fn)

        if self.pipe is not None:
            pipe = self.pipe
            self.pipe = None
            await asyncio.to_thread(pipe.close)

        self.connected = False

    async def send_envelope(self, kind: str, session_id: str, payload: dict[str, Any]) -> None:
        """Send one protocol v3 envelope to runtime."""

        self.sequence += 1
        envelope = {
            "v": IPC_PROTOCOL_V3,
            "channel": "trainer.stream",
            "msg_type": kind,
            "seq": self.sequence,
            "sent_at_us": now_micros(),
            "session_id": session_id,
            "request_id": None,
            "payload": payload,
        }
        await self._send_raw_message(envelope)

    async def receive_envelope(self) -> Optional[dict[str, Any]]:
        """Receive one envelope if available, else `None` on timeout."""

        if not self.connected:
            return None

        try:
            if self.config.ipc_mode == "tcp_loopback":
                if self.reader is None:
                    return None
                length_buf = await asyncio.wait_for(
                    self.reader.readexactly(4), timeout=self.config.recv_timeout_sec
                )
                length = struct.unpack("<I", length_buf)[0]
                body = await self.reader.readexactly(length)
            else:
                if self.ipc_channel is not None:
                    decoded = await asyncio.wait_for(
                        asyncio.to_thread(self.ipc_channel.recv_json),
                        timeout=self.config.recv_timeout_sec,
                    )
                    if isinstance(decoded, dict):
                        return decoded
                    return None
                if self.pipe is None:
                    return None
                # Named pipe reads are performed on a worker thread and may block
                # until runtime sends a frame. We avoid per-read timeouts here to
                # prevent spawning leaked background reads.
                length_buf = await asyncio.to_thread(self._pipe_read_exact, 4)
                length = struct.unpack("<I", length_buf)[0]
                body = await asyncio.to_thread(self._pipe_read_exact, length)

            decoded = json.loads(body.decode("utf-8"))
            if isinstance(decoded, dict):
                return decoded
            return None
        except asyncio.TimeoutError:
            return None
        except Exception as error:  # noqa: BLE001
            print(f"Bridge receive failed: {error}")
            self.connected = False
            return None

    def endpoint_label(self) -> str:
        return self.config.ipc_endpoint

    async def _send_raw_message(self, message: dict[str, Any]) -> None:
        if not self.connected:
            raise ConnectionError("Bridge is not connected")

        payload = json.dumps(message, separators=(",", ":")).encode("utf-8")
        frame = struct.pack("<I", len(payload)) + payload

        if self.config.ipc_mode == "tcp_loopback":
            if self.writer is None:
                raise ConnectionError("TCP writer unavailable")
            self.writer.write(frame)
            await self.writer.drain()
            return

        if self.ipc_channel is not None:
            await asyncio.to_thread(self.ipc_channel.send_json, message)
            return

        if self.pipe is None:
            raise ConnectionError("Named pipe handle unavailable")

        await asyncio.to_thread(self.pipe.write, frame)
        await asyncio.to_thread(self.pipe.flush)

    def _open_named_pipe_blocking(self) -> BinaryIO:
        deadline = time.monotonic() + self.config.connect_timeout_sec
        last_error: Optional[Exception] = None

        while time.monotonic() < deadline:
            try:
                # Unbuffered read/write binary mode works for byte-stream pipes.
                return open(self.config.ipc_endpoint, "r+b", buffering=0)
            except OSError as error:
                last_error = error
                time.sleep(0.1)

        raise TimeoutError(f"timed out opening named pipe {self.config.ipc_endpoint}: {last_error}")

    def _pipe_read_exact(self, size: int) -> bytes:
        if self.pipe is None:
            raise ConnectionError("Named pipe handle unavailable")

        data = bytearray()
        while len(data) < size:
            chunk = self.pipe.read(size - len(data))
            if not chunk:
                raise EOFError("named pipe closed")
            data.extend(chunk)
        return bytes(data)


class BridgeSession:
    """Stateful protocol v3 bridge session handler."""

    def __init__(self, client: IpcClient):
        self.client = client
        self.session_id = str(now_micros())
        self.stats = BridgeStats()
        self.errp = ErrPDetector(ErrPConfig())
        self.last_status_sent_at = 0.0

    async def run(self) -> None:
        """Run the connected bridge loop until disconnect/shutdown."""

        await self.send_hello()

        while self.client.connected:
            envelope = await self.client.receive_envelope()
            if envelope is not None:
                should_stop = await self.handle_runtime_message(envelope)
                if should_stop:
                    break

            now = time.monotonic()
            if now - self.last_status_sent_at >= self.client.config.heartbeat_interval_sec:
                await self.send_trainer_status()
                self.last_status_sent_at = now

            await asyncio.sleep(0.001)

    async def handle_runtime_message(self, envelope: dict[str, Any]) -> bool:
        """Handle one runtime->trainer envelope. Returns True to stop session."""

        version = int(envelope.get("v", 0))
        if version != IPC_PROTOCOL_V3:
            await self.send_protocol_error(
                code="unsupported_version",
                message=(
                    f"runtime sent unsupported protocol version {version}; "
                    f"expected {IPC_PROTOCOL_V3}"
                ),
                recoverable=True,
            )
            return False

        channel = str(envelope.get("channel", ""))
        if channel and channel != "trainer.stream":
            return False
        kind = str(envelope.get("msg_type", envelope.get("kind", "")))
        payload = envelope.get("payload")
        if not isinstance(payload, dict):
            payload = {}

        if kind == "hello":
            await self.send_ack("hello", int(envelope.get("seq", 0)))
            return False

        if kind == "session_boundary":
            event = str(payload.get("event", ""))
            if event == "start":
                self.stats = BridgeStats()
            return False

        if kind == "decision_event":
            await self.handle_decision_event(payload)
            return False

        if kind == "errp_window":
            await self.handle_errp_window(payload)
            return False

        if kind == "runtime_telemetry":
            return False

        if kind == "ping":
            await self.send_pong(payload)
            return False

        if kind == "shutdown":
            return True

        # Unknown runtime message kinds are recoverable.
        await self.send_protocol_error(
            code="unsupported_kind",
            message=f"trainer does not handle runtime message kind '{kind}'",
            recoverable=True,
        )
        return False

    async def handle_decision_event(self, payload: dict[str, Any]) -> None:
        _ = str(payload.get("decision_id") or f"dec_{now_micros()}")
        decoder_confidence = float(
            np.clip(float(payload.get("decoder_confidence") or 0.0), 0.0, 1.0)
        )
        signal_quality = float(np.clip(float(payload.get("signal_quality") or 0.0), 0.0, 1.0))

        feature_values = payload.get("feature_values")
        if isinstance(feature_values, list):
            np.asarray(feature_values, dtype=np.float32)

        self.stats.replay_size += 1
        self.stats.training_step += 1
        self.stats.decoder_confidence_ema = _ema(
            self.stats.decoder_confidence_ema, decoder_confidence, alpha=0.08
        )
        self.stats.signal_quality_ema = _ema(
            self.stats.signal_quality_ema, signal_quality, alpha=0.08
        )
        # Keep trainer status fields stable as replay grows, even before ErrP labels arrive.
        self.stats.policy_loss = _ema(
            self.stats.policy_loss, max(0.0, 1.0 - decoder_confidence), alpha=0.05
        )
        self.stats.entropy = _ema(
            self.stats.entropy, _bernoulli_entropy(decoder_confidence), alpha=0.05
        )

    async def handle_errp_window(self, payload: dict[str, Any]) -> None:
        decision_id = str(payload.get("decision_id") or f"dec_{now_micros()}")
        action_timestamp = int(payload.get("action_timestamp_us") or now_micros())
        signal_quality = float(np.clip(float(payload.get("signal_quality") or 0.0), 0.0, 1.0))

        channels = payload.get("channel_data")
        if isinstance(channels, list) and channels:
            try:
                matrix = np.asarray(channels, dtype=np.float32)
                if matrix.ndim == 2 and matrix.shape[0] > 0 and matrix.shape[1] > 0:
                    if self.errp.is_calibrated:
                        # Current detector expects [samples, channels].
                        detected = self.errp.detect(matrix.T)
                        error_probability = float(np.clip(detected.error_probability, 0.0, 1.0))
                        confidence = float(np.clip(detected.confidence, 0.0, 1.0))
                    else:
                        error_probability = 0.0
                        confidence = 0.0
                else:
                    error_probability = 0.0
                    confidence = 0.0
            except Exception as error:  # noqa: BLE001
                error_probability = 0.0
                confidence = 0.0
                self.stats.last_error = f"errp_window analysis failed: {error}"
        else:
            error_probability = 0.0
            confidence = 0.0

        self.stats.value_loss = _ema(self.stats.value_loss, error_probability, alpha=0.12)
        self.stats.policy_loss = _ema(
            self.stats.policy_loss, max(0.0, 1.0 - confidence), alpha=0.12
        )
        self.stats.entropy = _ema(
            self.stats.entropy, _bernoulli_entropy(error_probability), alpha=0.12
        )
        self.stats.last_error = None

        detection_timestamp = now_micros()
        await self.client.send_envelope(
            kind="errp_result",
            session_id=self.session_id,
            payload={
                "decision_id": decision_id,
                "action_timestamp_us": action_timestamp,
                "detection_timestamp_us": detection_timestamp,
                "error_probability": error_probability,
                "classification_confidence": confidence,
                "signal_quality": _quality_label(signal_quality),
                "estimated_magnitude": None,
                "detection_latency_us": detection_timestamp - action_timestamp,
            },
        )

    async def send_hello(self) -> None:
        await self.client.send_envelope(
            kind="hello",
            session_id=self.session_id,
            payload={
                "protocol": "neurohid_runtime_ml_v3",
                "role": "trainer",
                "capabilities": [
                    "errp_result",
                    "trainer_status",
                    "candidate_model_ready",
                ],
                "profile_id": None,
                "feature_schema_version": None,
                "action_schema_version": None,
                "decoder_model_version": None,
                "trainer_name": "neurohid-ml",
                "trainer_version": "0.1.0",
            },
        )

    async def send_pong(self, ping_payload: dict[str, Any]) -> None:
        await self.client.send_envelope(
            kind="pong",
            session_id=self.session_id,
            payload={
                "ping_id": str(ping_payload.get("ping_id") or ""),
                "timestamp_us": now_micros(),
            },
        )

    async def send_ack(self, ack_kind: str, ack_seq: int) -> None:
        await self.client.send_envelope(
            kind="ack",
            session_id=self.session_id,
            payload={
                "ack_kind": ack_kind,
                "ack_seq": ack_seq,
            },
        )

    async def send_protocol_error(self, code: str, message: str, recoverable: bool) -> None:
        self.stats.last_error = message
        await self.client.send_envelope(
            kind="error",
            session_id=self.session_id,
            payload={
                "code": code,
                "message": message,
                "recoverable": recoverable,
            },
        )

    async def send_trainer_status(self) -> None:
        if self.stats.last_error:
            state = "error"
        elif self.stats.replay_size > 0:
            state = "training"
        else:
            state = "idle"

        await self.client.send_envelope(
            kind="trainer_status",
            session_id=self.session_id,
            payload={
                "state": state,
                "replay_size": self.stats.replay_size,
                "training_step": self.stats.training_step,
                "policy_loss": self.stats.policy_loss,
                "value_loss": self.stats.value_loss,
                "entropy": self.stats.entropy,
                "last_error": self.stats.last_error,
            },
        )


async def main_async(
    ipc_mode: str | None = "local_socket",
    ipc_endpoint: str = DEFAULT_IPC_ENDPOINT,
    host: str = DEFAULT_HOST,
    port: int = DEFAULT_IPC_PORT,
    transport: str | IpcTransport | None = None,
    pipe_name: str = DEFAULT_PIPE_NAME,
) -> None:
    """Run reconnecting trainer bridge loop."""

    config = IpcConfig(
        ipc_mode=ipc_mode,
        ipc_endpoint=ipc_endpoint,
        host=host,
        port=port,
        pipe_name=pipe_name,
        transport=(
            transport
            if transport is not None
            else (IpcTransport.NAMED_PIPE if os.name == "nt" else IpcTransport.TCP_LOOPBACK)
        ),
    )

    attempts = 0
    while True:
        client = IpcClient(config)
        print(f"Connecting trainer bridge to {client.endpoint_label()}...")
        connected = await client.connect()

        if connected:
            attempts = 0
            print("Trainer bridge connected")
            session = BridgeSession(client)
            try:
                await session.run()
            finally:
                await client.disconnect()
            print("Trainer bridge disconnected")
        else:
            attempts += 1

        if not config.auto_reconnect:
            return
        if config.max_reconnect_attempts > 0 and attempts >= config.max_reconnect_attempts:
            print("Max reconnect attempts reached; exiting")
            return

        await asyncio.sleep(config.reconnect_delay_sec)


def main() -> None:
    parser = argparse.ArgumentParser(description="NeuroHID runtime ML bridge (v3)")
    parser.add_argument(
        "--ipc-mode",
        choices=["local_socket", "tcp_loopback"],
        default="local_socket",
        help="canonical IPC mode (preferred)",
    )
    parser.add_argument(
        "--ipc-endpoint",
        default=DEFAULT_IPC_ENDPOINT,
        help="canonical endpoint path/name or host:port (preferred)",
    )
    parser.add_argument(
        "--transport",
        choices=[transport.value for transport in IpcTransport],
        default=(
            IpcTransport.NAMED_PIPE.value if os.name == "nt" else IpcTransport.TCP_LOOPBACK.value
        ),
        help="legacy transport alias (prefer --ipc-mode/--ipc-endpoint)",
    )
    parser.add_argument("--host", default=DEFAULT_HOST, help="legacy alias")
    parser.add_argument("--port", type=int, default=DEFAULT_IPC_PORT, help="legacy alias")
    parser.add_argument("--pipe-name", default=DEFAULT_PIPE_NAME, help="legacy alias")

    args = parser.parse_args()
    asyncio.run(
        main_async(
            ipc_mode=args.ipc_mode,
            ipc_endpoint=args.ipc_endpoint,
            host=args.host,
            port=args.port,
            transport=args.transport,
            pipe_name=args.pipe_name,
        )
    )


if __name__ == "__main__":
    main()
