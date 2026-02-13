from __future__ import annotations

import io
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from neurohid_ml.lab_kernel import LabKernel, run_stdio


class LabKernelTests(unittest.TestCase):
    def test_execute_returns_last_expression_repr(self) -> None:
        kernel = LabKernel()
        result = kernel.execute("x = 2\nx + 3")

        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["result"], "5")
        self.assertEqual(result["exec_count"], 1)

    def test_reset_clears_execution_state(self) -> None:
        kernel = LabKernel()
        kernel.execute("x = 7")
        kernel.reset()

        result = kernel.execute("x")

        self.assertEqual(result["status"], "error")
        self.assertEqual(result["error"]["name"], "NameError")

    def test_run_stdio_handles_execute_ping_and_shutdown(self) -> None:
        input_lines = (
            '{"type":"execute","request_id":1,"code":"print(42)"}\n'
            '{"type":"ping","request_id":2}\n'
            '{"type":"shutdown"}\n'
        )
        original_stdin = sys.stdin
        original_stdout = sys.stdout
        captured = io.StringIO()
        sys.stdin = io.StringIO(input_lines)
        sys.stdout = captured
        try:
            run_stdio()
        finally:
            sys.stdin = original_stdin
            sys.stdout = original_stdout

        responses = [json.loads(line) for line in captured.getvalue().splitlines()]
        self.assertEqual(responses[0]["type"], "ready")
        self.assertEqual(responses[1]["type"], "execute_result")
        self.assertEqual(responses[1]["stdout"], "42\n")
        self.assertEqual(responses[2], {"type": "pong", "request_id": 2})

    def test_run_stdio_emits_error_for_invalid_request(self) -> None:
        input_lines = (
            '{"type":"execute","request_id":9,"code":3}\n'
            '{"type":"shutdown"}\n'
        )
        original_stdin = sys.stdin
        original_stdout = sys.stdout
        captured = io.StringIO()
        sys.stdin = io.StringIO(input_lines)
        sys.stdout = captured
        try:
            run_stdio()
        finally:
            sys.stdin = original_stdin
            sys.stdout = original_stdout

        responses = [json.loads(line) for line in captured.getvalue().splitlines()]
        self.assertEqual(responses[1]["type"], "error")
        self.assertEqual(responses[1]["request_id"], 9)
        self.assertIn("string field 'code'", responses[1]["message"])


if __name__ == "__main__":
    unittest.main()
