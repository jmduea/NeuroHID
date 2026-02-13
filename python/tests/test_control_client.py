from __future__ import annotations

import importlib
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

_control = importlib.import_module("neurohid_ml.control")


class ControlClientTests(unittest.TestCase):
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
            with self.assertRaisesRegex(_control.NotebookError, "unable to reach NeuroHID"):
                client._request_endpoint('{"request_id":null,"command":{"type":"snapshot"}}\n')

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
        self.assertFalse(
            _control._is_eligible_eeg_stream(  # noqa: SLF001 - testing module helper
                {
                    "stream_type": "AUX/ACC",
                    "channel_count": 3,
                    "sample_rate": 32.0,
                }
            )
        )


if __name__ == "__main__":
    unittest.main()
