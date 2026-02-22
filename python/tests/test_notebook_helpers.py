from __future__ import annotations

import importlib
import json
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

_notebook = importlib.import_module("neurohid_ml.notebook")


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

    def subscribe_events(self):
        return iter([])


class NotebookHelperTests(unittest.TestCase):
    def test_set_fallback_policy_passes_through(self) -> None:
        runtime = _FakeRuntime()
        notebook = _notebook.NeuroHidNotebook(runtime=runtime)

        policy = {"enabled": True, "model_strategy": "lightweight_rust"}
        response = notebook.set_fallback_policy(policy)
        self.assertEqual(response, {"status": "ok"})

    def test_snapshot_returns_dict(self) -> None:
        runtime = _FakeRuntime()
        notebook = _notebook.NeuroHidNotebook(runtime=runtime)
        snap = notebook.snapshot()
        self.assertIsInstance(snap, dict)
        self.assertTrue(snap["running"])

    def test_set_output_enabled_sends_command(self) -> None:
        runtime = _FakeRuntime()
        notebook = _notebook.NeuroHidNotebook(runtime=runtime)
        result = notebook.set_output_enabled(True)
        self.assertEqual(result, {"status": "ok"})
        self.assertEqual(runtime.commands, [("toggle_output", {"enabled": True})])

    def test_train_profile_candidate_builds_expected_command(self) -> None:
        runtime = _FakeRuntime()
        notebook = _notebook.NeuroHidNotebook(
            runtime=runtime,
            service_bin="custom-service",
        )

        with patch.object(
            _notebook.NeuroHidNotebook,
            "_run_command",
            autospec=True,
            return_value=subprocess.CompletedProcess(
                args=["ok"], returncode=0, stdout=""
            ),
        ) as run_command:
            notebook.train_profile_candidate(
                "profile-1",
                epochs=2,
                learning_rate=0.01,
                min_samples=8,
            )

        called_command = run_command.call_args[0][1]
        self.assertIn("train-profile-candidate", called_command)
        self.assertIn("--profile-id", called_command)
        self.assertIn("profile-1", called_command)
        self.assertIn("--service-bin", called_command)
        self.assertIn("custom-service", called_command)
        self.assertIn("--min-samples", called_command)
        self.assertIn("8", called_command)

    def test_run_command_raises_notebook_error_with_process_output(self) -> None:
        runtime = _FakeRuntime()
        notebook = _notebook.NeuroHidNotebook(runtime=runtime)
        failed = subprocess.CompletedProcess(
            args=["bad"],
            returncode=2,
            stdout="stdout message",
            stderr="stderr message",
        )
        with patch.object(
            _notebook.subprocess,
            "run",
            autospec=True,
            return_value=failed,
        ):
            with self.assertRaisesRegex(_notebook.NotebookError, "stdout message"):
                notebook._run_command(["bad"])

    def test_telemetry_client_returns_telemetry_wrapper(self) -> None:
        runtime = _FakeRuntime()
        notebook = _notebook.NeuroHidNotebook(runtime=runtime)
        client = notebook.telemetry_client()
        self.assertTrue(client.is_alive())


if __name__ == "__main__":
    unittest.main()
