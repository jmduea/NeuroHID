from __future__ import annotations

import importlib
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

_control = importlib.import_module("neurohid_ml.control")


class _FakeSnapshot:
    def to_dict(self) -> dict:
        return {
            "running": True,
            "discovered_streams": [
                {
                    "id": "stream-1",
                    "stream_type": "EEG/EmotivEEG",
                    "channel_count": 5,
                    "sample_rate": 128.0,
                    "connected": False,
                }
            ],
        }


class _FakeRuntime:
    def __init__(self) -> None:
        self.commands: list[tuple] = []
        self._dispatch_response = '{"status":"ok"}'

    def snapshot(self) -> _FakeSnapshot:
        return _FakeSnapshot()

    def trainer_snapshot(self) -> _FakeSnapshot:
        return _FakeSnapshot()

    def command(self, name: str, **kwargs) -> None:
        self.commands.append((name, kwargs))

    def dispatch_control_sync(self, request_json: str) -> str:
        return self._dispatch_response

    def is_alive(self) -> bool:
        return True

    def subscribe_samples(self):
        return iter([])

    def subscribe_features(self):
        return iter([])

    def subscribe_actions(self):
        return iter([])

    def subscribe_markers(self):
        return iter([])

    def subscribe_events(self):
        return iter([])


class ControlClientTests(unittest.TestCase):
    def test_snapshot_returns_dict(self) -> None:
        client = _control.NeuroHidControlClient(_FakeRuntime())
        snap = client.snapshot()
        self.assertIsInstance(snap, dict)
        self.assertTrue(snap["running"])

    def test_trainer_snapshot_returns_dict(self) -> None:
        client = _control.NeuroHidControlClient(_FakeRuntime())
        snap = client.trainer_snapshot()
        self.assertIsInstance(snap, dict)

    def test_set_output_enabled_sends_toggle_command(self) -> None:
        runtime = _FakeRuntime()
        client = _control.NeuroHidControlClient(runtime)
        result = client.set_output_enabled(True)
        self.assertEqual(result, {"status": "ok"})
        self.assertEqual(runtime.commands, [("toggle_output", {"enabled": True})])

    def test_set_learning_enabled_sends_command(self) -> None:
        runtime = _FakeRuntime()
        client = _control.NeuroHidControlClient(runtime)
        result = client.set_learning_enabled(False)
        self.assertEqual(result, {"status": "ok"})
        self.assertEqual(runtime.commands, [("set_learning_enabled", {"enabled": False})])

    def test_rescan_streams_sends_command(self) -> None:
        runtime = _FakeRuntime()
        client = _control.NeuroHidControlClient(runtime)
        result = client.rescan_streams()
        self.assertEqual(result, {"status": "ok"})
        self.assertEqual(runtime.commands, [("rescan_streams", {})])

    def test_connect_stream_sends_command(self) -> None:
        runtime = _FakeRuntime()
        client = _control.NeuroHidControlClient(runtime)
        result = client.connect_stream("stream-abc")
        self.assertEqual(result, {"status": "ok"})
        self.assertEqual(runtime.commands, [("connect_stream", {"stream_id": "stream-abc"})])

    def test_set_fallback_policy_dispatches_control(self) -> None:
        runtime = _FakeRuntime()
        client = _control.NeuroHidControlClient(runtime)
        policy = {"enabled": True, "model_strategy": "lightweight_rust"}
        result = client.set_fallback_policy(policy)
        self.assertEqual(result, {"status": "ok"})

    def test_set_fallback_policy_rejects_non_dict(self) -> None:
        runtime = _FakeRuntime()
        client = _control.NeuroHidControlClient(runtime)
        with self.assertRaises(_control.NotebookError):
            client.set_fallback_policy("not a dict")  # type: ignore[arg-type]

    def test_is_alive_delegates_to_runtime(self) -> None:
        runtime = _FakeRuntime()
        client = _control.NeuroHidControlClient(runtime)
        self.assertTrue(client.is_alive())

    def test_ensure_connected_stream_connects_eligible(self) -> None:
        runtime = _FakeRuntime()
        client = _control.NeuroHidControlClient(runtime)
        stream_id = client.ensure_connected_stream(rescan=False)
        self.assertEqual(stream_id, "stream-1")
        self.assertEqual(runtime.commands, [("connect_stream", {"stream_id": "stream-1"})])

    def test_eligible_eeg_stream_filter(self) -> None:
        self.assertTrue(
            _control._is_eligible_eeg_stream(
                {
                    "stream_type": "EEG/EmotivEEG",
                    "channel_count": 5,
                    "sample_rate": 128.0,
                }
            )
        )
        self.assertFalse(
            _control._is_eligible_eeg_stream(
                {
                    "stream_type": "AUX/ACC",
                    "channel_count": 3,
                    "sample_rate": 32.0,
                }
            )
        )


if __name__ == "__main__":
    unittest.main()
