from __future__ import annotations

import importlib
import io
import json
import struct
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

_cli = importlib.import_module("neurohid_ml.cli")
_control = importlib.import_module("neurohid_ml.control")
_telemetry = importlib.import_module("neurohid_ml.telemetry")


class CliAndClientTests(unittest.TestCase):
    def test_control_send_command_raises_on_error_payload(self) -> None:
        client = _control.NeuroHidControlClient(auto_start_service=False)

        def fake_request(_: str) -> str:
            return json.dumps({"payload": {"type": "error", "message": "boom"}})

        with patch.object(
            _control.NeuroHidControlClient,
            "_request_endpoint",
            autospec=True,
            side_effect=lambda _self, _payload: fake_request(_payload),
        ):
            with self.assertRaisesRegex(_control.NotebookError, "boom"):
                client.send_command({"type": "snapshot"})

    def test_control_send_command_returns_response(self) -> None:
        client = _control.NeuroHidControlClient(auto_start_service=False)

        def fake_request(_: str) -> str:
            return json.dumps({"payload": {"type": "ack"}})

        with patch.object(
            _control.NeuroHidControlClient,
            "_request_endpoint",
            autospec=True,
            side_effect=lambda _self, _payload: fake_request(_payload),
        ):
            response = client.send_command({"type": "set_output_enabled", "enabled": True})
            self.assertEqual(response["payload"]["type"], "ack")

    def test_parse_args_control_subcommand(self) -> None:
        args = _cli._parse_args(
            [
                "control",
                "set_output_enabled",
                "--enabled",
                "true",
                "--transport",
                "tcp",
                "--host",
                "127.0.0.1",
                "--port",
                "47385",
            ]
        )

        self.assertEqual(args.command, "control")
        self.assertEqual(args.action, "set_output_enabled")
        self.assertTrue(args.enabled)
        self.assertEqual(args.transport, "tcp")

    def test_parse_args_control_fallback_policy(self) -> None:
        policy_json = '{"enabled":true,"model_strategy":"lightweight_rust"}'
        args = _cli._parse_args(
            [
                "control",
                "set_fallback_policy",
                "--policy-json",
                policy_json,
            ]
        )

        self.assertEqual(args.command, "control")
        self.assertEqual(args.action, "set_fallback_policy")
        self.assertEqual(args.policy_json, policy_json)

    def test_parse_args_telemetry_subcommand(self) -> None:
        args = _cli._parse_args(
            [
                "telemetry-read",
                "--transport",
                "tcp_loopback",
                "--max-messages",
                "3",
            ]
        )

        self.assertEqual(args.command, "telemetry-read")
        self.assertEqual(args.transport, "tcp_loopback")
        self.assertEqual(args.max_messages, 3)

    def test_read_framed_json_decodes_envelope(self) -> None:
        envelope = {"v": 2, "kind": "trainer_status", "payload": {"state": "idle"}}
        payload = json.dumps(envelope).encode("utf-8")
        frame = struct.pack("<I", len(payload)) + payload

        reader = io.BytesIO(frame)
        decoded = _telemetry._read_framed_json(reader)

        self.assertEqual(decoded, envelope)

    def test_control_set_fallback_policy_sends_policy(self) -> None:
        client = _control.NeuroHidControlClient(auto_start_service=False)
        expected_policy = {"enabled": True, "model_strategy": "lightweight_rust"}

        with patch.object(
            _control.NeuroHidControlClient,
            "send_command",
            autospec=True,
            return_value={"payload": {"type": "ack"}},
        ) as send_command:
            response = client.set_fallback_policy(expected_policy)

        self.assertEqual(response["payload"]["type"], "ack")
        send_command.assert_called_once_with(
            client,
            {
                "type": "set_fallback_policy",
                "policy": expected_policy,
            },
        )


if __name__ == "__main__":
    unittest.main()
