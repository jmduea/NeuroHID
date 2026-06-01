from __future__ import annotations

import importlib
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

_cli = importlib.import_module("neurohid_ml.cli")
_control = importlib.import_module("neurohid_ml.control")


class _FakeSnapshot:
    def to_dict(self) -> dict:
        return {"running": True, "discovered_streams": []}


class _FakeRuntime:
    def __init__(self) -> None:
        self.commands: list[tuple] = []

    def snapshot(self) -> _FakeSnapshot:
        return _FakeSnapshot()

    def trainer_snapshot(self) -> _FakeSnapshot:
        return _FakeSnapshot()

    def command(self, name: str, **kwargs) -> None:
        self.commands.append((name, kwargs))

    def dispatch_control_sync(self, request_json: str) -> str:
        return json.dumps({"status": "ok"})

    def is_alive(self) -> bool:
        return True


class CliParseTests(unittest.TestCase):
    def test_parse_args_bridge_defaults(self) -> None:
        args = _cli._parse_args(["bridge"])
        self.assertEqual(args.command, "bridge")

    def test_parse_args_control_subcommand(self) -> None:
        args = _cli._parse_args(
            [
                "control",
                "set_output_enabled",
                "--enabled",
                "true",
            ]
        )
        self.assertEqual(args.command, "control")
        self.assertEqual(args.action, "set_output_enabled")
        self.assertTrue(args.enabled)

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


class ControlClientTests(unittest.TestCase):
    def test_snapshot_delegates_to_runtime(self) -> None:
        runtime = _FakeRuntime()
        client = _control.NeuroHidControlClient(runtime)
        snap = client.snapshot()
        self.assertIn("running", snap)

    def test_set_output_enabled_sends_command(self) -> None:
        runtime = _FakeRuntime()
        client = _control.NeuroHidControlClient(runtime)
        result = client.set_output_enabled(True)
        self.assertEqual(result, {"status": "ok"})
        self.assertEqual(runtime.commands, [("toggle_output", {"enabled": True})])

    def test_set_fallback_policy_sends_dispatch_control(self) -> None:
        runtime = _FakeRuntime()
        client = _control.NeuroHidControlClient(runtime)
        policy = {"enabled": True, "model_strategy": "lightweight_rust"}
        result = client.set_fallback_policy(policy)
        self.assertEqual(result, {"status": "ok"})


if __name__ == "__main__":
    unittest.main()
