from __future__ import annotations

import importlib
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

_notebook = importlib.import_module("neurohid_ml.notebook")


class _FakeControl:
    def __init__(self) -> None:
        self.calls: list[tuple[str, object]] = []

    def set_fallback_policy(self, policy: dict) -> dict:
        self.calls.append(("set_fallback_policy", policy))
        return {"payload": {"type": "ack"}}


class NotebookHelperTests(unittest.TestCase):
    def test_set_fallback_policy_passes_through(self) -> None:
        notebook = _notebook.NeuroHidNotebook(auto_start_service=False)
        fake_control = _FakeControl()
        notebook._control = fake_control  # type: ignore[assignment]

        policy = {"enabled": True, "model_strategy": "lightweight_rust"}
        response = notebook.set_fallback_policy(policy)

        self.assertEqual(response["payload"]["type"], "ack")
        self.assertEqual(fake_control.calls, [("set_fallback_policy", policy)])

    def test_train_profile_candidate_builds_expected_command(self) -> None:
        notebook = _notebook.NeuroHidNotebook(
            auto_start_service=False,
            service_bin="custom-service",
        )

        with patch.object(
            _notebook.NeuroHidNotebook,
            "_run_command",
            autospec=True,
            return_value=subprocess.CompletedProcess(args=["ok"], returncode=0, stdout=""),
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
        notebook = _notebook.NeuroHidNotebook(auto_start_service=False)
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

    def test_subscribe_events_with_reconnect_uses_resume_cursor(self) -> None:
        notebook = _notebook.NeuroHidNotebook(auto_start_service=False)

        class _FlakyControl:
            def __init__(self) -> None:
                self.calls = 0
                self.resume_args: list[int | None] = []

            def subscribe_events(self, **kwargs):  # type: ignore[no-untyped-def]
                self.calls += 1
                self.resume_args.append(kwargs.get("resume_from_seq"))
                if self.calls == 1:
                    raise _notebook.NotebookError("temporary disconnect")
                return iter(
                    [
                        {"type": "sample", "_seq": 11},
                        {"type": "sample", "_seq": 12},
                    ]
                )

        flaky = _FlakyControl()
        notebook._control = flaky  # type: ignore[assignment]
        events = list(
            notebook.subscribe_events_with_reconnect(
                max_messages=2,
                families=["sample"],
                reconnect_attempts=2,
                reconnect_backoff_secs=0.0,
            )
        )

        self.assertEqual(events[0]["_seq"], 11)
        self.assertEqual(events[1]["_seq"], 12)
        self.assertEqual(flaky.resume_args, [None, None])


if __name__ == "__main__":
    unittest.main()
