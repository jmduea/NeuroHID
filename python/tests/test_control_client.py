from __future__ import annotations

import importlib
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

_control = importlib.import_module("neurohid_ml.control")


class ControlClientTests(unittest.TestCase):
    def test_default_ipc_endpoint_matches_canonical(self) -> None:
        client = _control.NeuroHidControlClient(auto_start_service=False)
        self.assertEqual(client.ipc_endpoint, "neurohid.control.v3")

    def test_service_launch_commands_include_cargo_fallback(self) -> None:
        client = _control.NeuroHidControlClient(auto_start_service=False)
        commands = client._service_launch_commands()

        self.assertGreaterEqual(len(commands), 2)
        self.assertEqual(commands[0][0][0], client.service_bin)
        self.assertTrue(any(cmd[0][0] == "cargo" for cmd in commands))

    def test_request_endpoint_error_mentions_endpoint(self) -> None:
        client = _control.NeuroHidControlClient(auto_start_service=False)
        with patch.object(
            _control.NeuroHidControlClient,
            "_try_configured_endpoint",
            autospec=True,
            return_value=(OSError("boom"), None),
        ):
            with self.assertRaisesRegex(
                _control.NotebookError, "unable to reach NeuroHID"
            ):
                client._request_endpoint(
                    '{"request_id":null,"command":{"type":"snapshot"}}\n'
                )

    def test_eligible_eeg_stream_filter(self) -> None:
        self.assertTrue(
            _control._is_eligible_eeg_stream(  # noqa: SLF001 - testing module helper
                {
                    "stream_type": "EEG/EmotivEEG",
                    "channel_count": 5,
                    "sample_rate": 128.0,
                }
            )
        )

    def test_daemon_status_invokes_service_binary(self) -> None:
        client = _control.NeuroHidControlClient(
            auto_start_service=False,
            service_bin="svc-bin",
        )
        completed = _control.subprocess.CompletedProcess(
            args=["svc-bin"],
            returncode=0,
            stdout="status=running",
            stderr="",
        )
        with patch.object(
            _control.subprocess,
            "run",
            autospec=True,
            return_value=completed,
        ) as run_call:
            response = client.daemon_status()

        called = run_call.call_args.args[0]
        self.assertEqual(
            called,
            ["svc-bin", "daemon", "status"],
        )
        self.assertEqual(response["payload"]["type"], "daemon_status")
        self.assertFalse(
            _control._is_eligible_eeg_stream(  # noqa: SLF001 - testing module helper
                {
                    "stream_type": "AUX/ACC",
                    "channel_count": 3,
                    "sample_rate": 32.0,
                }
            )
        )

    def test_subscribe_events_forwards_stream_options(self) -> None:
        client = _control.NeuroHidControlClient(auto_start_service=False)
        ipc_client = unittest.mock.Mock()
        ipc_client.iter_runtime_events.return_value = iter([{"type": "snapshot"}])
        with patch.object(
            _control.NeuroHidControlClient,
            "_build_ipc_client",
            autospec=True,
            return_value=ipc_client,
        ):
            events = list(
                client.subscribe_events(
                    max_messages=1,
                    families=["sample", "feature_frame"],
                    resume_from_seq=42,
                    sample_every=3,
                    max_duration_ms=2_000,
                    snapshot_interval_ms=500,
                    prefer_stream=False,
                )
            )

        self.assertEqual(events, [{"type": "snapshot"}])
        ipc_client.iter_runtime_events.assert_called_once_with(
            max_messages=1,
            families=["sample", "feature_frame"],
            resume_from_seq=42,
            sample_every=3,
            max_duration_ms=2_000,
            snapshot_interval_ms=500,
            prefer_stream=False,
        )


if __name__ == "__main__":
    unittest.main()
