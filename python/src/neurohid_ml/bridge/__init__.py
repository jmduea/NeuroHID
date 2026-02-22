"""Runtime ML bridge (protocol v3) for NeuroHID — in-process variant.

This module implements the trainer-side endpoint for the NeuroHID runtime ML
protocol v3.  Communication goes through the in-process ``RuntimeHandle``
trainer bridge methods instead of sockets/pipes.
"""

from __future__ import annotations

import asyncio
import json
import time
from dataclasses import dataclass
from typing import Any, Optional

import numpy as np

from neurohid_ml.errp import ErrPConfig, ErrPDetector

IPC_PROTOCOL_VERSION = 3


@dataclass
class IpcConfig:
    """Configuration for the trainer bridge loop."""

    heartbeat_interval_sec: float = 0.5
    recv_timeout_sec: float = 0.2


@dataclass
class BridgeStats:
    """Lightweight trainer runtime stats surfaced via ``trainer_status``."""

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
    """Trainer-side bridge client backed by an in-process ``RuntimeHandle``.

    Parameters
    ----------
    runtime:
        A ``neurohid.RuntimeHandle`` obtained from ``await RuntimeBuilder(...).start()``.
    config:
        Optional bridge configuration (heartbeat interval, etc.).
    """

    def __init__(self, runtime: Any, config: IpcConfig | None = None) -> None:
        self.config = config or IpcConfig()
        self._runtime = runtime
        self.connected = False
        self.sequence = 0

    async def connect(self) -> bool:
        """Register a trainer session with the runtime."""
        try:
            session_id = str(now_micros())
            await self._runtime.trainer_connect(session_id)
            self.connected = True
            return True
        except Exception as error:  # noqa: BLE001
            print(f"Bridge connect failed: {error}")
            self.connected = False
            return False

    async def disconnect(self) -> None:
        """Disconnect the trainer session."""
        if self.connected:
            try:
                await self._runtime.trainer_disconnect()
            except Exception:  # noqa: BLE001
                pass
        self.connected = False

    async def send_envelope(
        self, kind: str, session_id: str, payload: dict[str, Any]
    ) -> None:
        """Send one protocol v3 envelope to the runtime via the trainer bridge."""
        self.sequence += 1
        envelope = {
            "v": IPC_PROTOCOL_VERSION,
            "channel": "trainer.stream",
            "msg_type": kind,
            "seq": self.sequence,
            "sent_at_us": now_micros(),
            "session_id": session_id,
            "request_id": None,
            "payload": payload,
        }
        await self._runtime.trainer_send(envelope)

    async def receive_envelope(self) -> Optional[dict[str, Any]]:
        """Receive one envelope from the runtime, or ``None`` on timeout/close."""
        if not self.connected:
            return None
        try:
            result = await asyncio.wait_for(
                self._runtime.trainer_recv(),
                timeout=self.config.recv_timeout_sec,
            )
            if result is None:
                return None
            return json.loads(result)
        except asyncio.TimeoutError:
            return None
        except Exception as error:  # noqa: BLE001
            print(f"Bridge receive failed: {error}")
            self.connected = False
            return None

    def endpoint_label(self) -> str:
        return "in-process"


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
            if (
                now - self.last_status_sent_at
                >= self.client.config.heartbeat_interval_sec
            ):
                await self.send_trainer_status()
                self.last_status_sent_at = now

            await asyncio.sleep(0.001)

    async def handle_runtime_message(self, envelope: dict[str, Any]) -> bool:
        """Handle one runtime->trainer envelope. Returns True to stop session."""
        version = int(envelope.get("v", 0))
        if version != IPC_PROTOCOL_VERSION:
            await self.send_protocol_error(
                code="unsupported_version",
                message=(
                    f"runtime sent unsupported protocol version {version}; "
                    f"expected {IPC_PROTOCOL_VERSION}"
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
        signal_quality = float(
            np.clip(float(payload.get("signal_quality") or 0.0), 0.0, 1.0)
        )

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
        self.stats.policy_loss = _ema(
            self.stats.policy_loss, max(0.0, 1.0 - decoder_confidence), alpha=0.05
        )
        self.stats.entropy = _ema(
            self.stats.entropy, _bernoulli_entropy(decoder_confidence), alpha=0.05
        )

    async def handle_errp_window(self, payload: dict[str, Any]) -> None:
        decision_id = str(payload.get("decision_id") or f"dec_{now_micros()}")
        action_timestamp = int(payload.get("action_timestamp_us") or now_micros())
        signal_quality = float(
            np.clip(float(payload.get("signal_quality") or 0.0), 0.0, 1.0)
        )

        channels = payload.get("channel_data")
        if isinstance(channels, list) and channels:
            try:
                matrix = np.asarray(channels, dtype=np.float32)
                if matrix.ndim == 2 and matrix.shape[0] > 0 and matrix.shape[1] > 0:
                    if self.errp.is_calibrated:
                        detected = self.errp.detect(matrix.T)
                        error_probability = float(
                            np.clip(detected.error_probability, 0.0, 1.0)
                        )
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

        self.stats.value_loss = _ema(
            self.stats.value_loss, error_probability, alpha=0.12
        )
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

    async def send_protocol_error(
        self, code: str, message: str, recoverable: bool
    ) -> None:
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


async def main_async(runtime: Any) -> None:
    """Run the trainer bridge loop against an in-process runtime handle."""
    client = IpcClient(runtime)
    print("Connecting trainer bridge (in-process)...")
    connected = await client.connect()

    if not connected:
        print("Trainer bridge connect failed")
        return

    print("Trainer bridge connected")
    session = BridgeSession(client)
    try:
        await session.run()
    finally:
        await client.disconnect()
    print("Trainer bridge disconnected")
