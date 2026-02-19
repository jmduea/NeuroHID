from __future__ import annotations

import importlib
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

_bridge = importlib.import_module("neurohid_ml.bridge")


class _FakeBridgeClient:
    def __init__(self) -> None:
        self.config = SimpleNamespace(heartbeat_interval_sec=999.0)
        self.connected = True
        self.sent: list[dict] = []

    async def send_envelope(self, kind: str, session_id: str, payload: dict) -> None:
        self.sent.append(
            {
                "kind": kind,
                "session_id": session_id,
                "payload": payload,
            }
        )


class BridgeSessionTests(unittest.IsolatedAsyncioTestCase):
    async def test_unsupported_version_emits_protocol_error(self) -> None:
        client = _FakeBridgeClient()
        session = _bridge.BridgeSession(client)

        should_stop = await session.handle_runtime_message(
            {
                "v": 1,
                "channel": "trainer.stream",
                "msg_type": "hello",
                "seq": 1,
                "payload": {},
            }
        )

        self.assertFalse(should_stop)
        self.assertEqual(client.sent[-1]["kind"], "error")
        self.assertEqual(
            client.sent[-1]["payload"]["code"],
            "unsupported_version",
        )

    async def test_ping_emits_pong(self) -> None:
        client = _FakeBridgeClient()
        session = _bridge.BridgeSession(client)

        should_stop = await session.handle_runtime_message(
            {
                "v": 3,
                "channel": "trainer.stream",
                "msg_type": "ping",
                "payload": {"ping_id": "abc"},
            }
        )

        self.assertFalse(should_stop)
        self.assertEqual(client.sent[-1]["kind"], "pong")
        self.assertEqual(client.sent[-1]["payload"]["ping_id"], "abc")

    async def test_shutdown_returns_true(self) -> None:
        client = _FakeBridgeClient()
        session = _bridge.BridgeSession(client)

        should_stop = await session.handle_runtime_message(
            {"v": 3, "channel": "trainer.stream", "msg_type": "shutdown", "payload": {}}
        )

        self.assertTrue(should_stop)

    async def test_decision_event_updates_stats(self) -> None:
        client = _FakeBridgeClient()
        session = _bridge.BridgeSession(client)

        await session.handle_runtime_message(
            {
                "v": 3,
                "channel": "trainer.stream",
                "msg_type": "decision_event",
                "payload": {"decoder_confidence": 0.7, "signal_quality": 0.8},
            }
        )

        self.assertEqual(session.stats.replay_size, 1)
        self.assertEqual(session.stats.training_step, 1)
        self.assertIsNotNone(session.stats.policy_loss)
        self.assertIsNotNone(session.stats.entropy)

    async def test_errp_window_emits_result_even_without_calibration(self) -> None:
        client = _FakeBridgeClient()
        session = _bridge.BridgeSession(client)

        await session.handle_runtime_message(
            {
                "v": 3,
                "channel": "trainer.stream",
                "msg_type": "errp_window",
                "payload": {
                    "decision_id": "d-1",
                    "action_timestamp_us": 10,
                    "signal_quality": 0.9,
                    "channel_data": [[0.1, 0.2, 0.3], [0.1, 0.2, 0.3]],
                },
            }
        )

        self.assertEqual(client.sent[-1]["kind"], "errp_result")
        self.assertEqual(client.sent[-1]["payload"]["decision_id"], "d-1")
        self.assertEqual(client.sent[-1]["payload"]["error_probability"], 0.0)


class BridgeConfigTests(unittest.TestCase):
    def test_tcp_mode_uses_canonical_default_port(self) -> None:
        config = _bridge.IpcConfig(ipc_mode="tcp_loopback", ipc_endpoint="")
        self.assertEqual(config.host, "127.0.0.1")
        self.assertEqual(config.port, 47_384)


if __name__ == "__main__":
    unittest.main()
