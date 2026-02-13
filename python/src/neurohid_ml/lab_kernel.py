"""Notebook-style kernel adapter for NeuroHID Python Lab.

Protocol: newline-delimited JSON over stdio.

Requests:
- {"type": "execute", "request_id": <u64>, "code": "..."}
- {"type": "reset", "request_id": <u64>}
- {"type": "ping", "request_id": <u64>}
- {"type": "shutdown"}

Responses:
- {"type": "ready", "protocol": "neurohid_lab_kernel_v1"}
- {"type": "execute_result", ...}
- {"type": "reset_result", "request_id": ...}
- {"type": "pong", "request_id": ...}
- {"type": "error", "request_id": <u64|null>, "message": "..."}
"""

from __future__ import annotations

import ast
import contextlib
import io
import json
import sys
import time
import traceback
from dataclasses import dataclass
from typing import Any

PROTOCOL_VERSION = "neurohid_lab_kernel_v1"


@dataclass
class ExecuteError:
    name: str
    message: str
    traceback_text: str

    def to_payload(self) -> dict[str, str]:
        return {
            "name": self.name,
            "message": self.message,
            "traceback": self.traceback_text,
        }


class LabKernel:
    """Stateful code execution kernel with notebook-like semantics."""

    def __init__(self) -> None:
        self.exec_count = 0
        self.reset()

    def reset(self) -> None:
        self.globals: dict[str, Any] = {
            "__name__": "__neurohid_lab__",
            "__package__": None,
            "__builtins__": __builtins__,
        }

    def execute(self, code: str) -> dict[str, Any]:
        start_ns = time.time_ns()
        stdout = io.StringIO()
        stderr = io.StringIO()

        status = "ok"
        result_repr: str | None = None
        error_payload: dict[str, str] | None = None

        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            try:
                result_repr = self._execute_with_expression_result(code)
            except Exception as exc:  # noqa: BLE001 - kernel must not crash caller
                status = "error"
                error_payload = ExecuteError(
                    name=type(exc).__name__,
                    message=str(exc),
                    traceback_text=traceback.format_exc(),
                ).to_payload()

        self.exec_count += 1
        duration_ms = int((time.time_ns() - start_ns) / 1_000_000)

        return {
            "status": status,
            "stdout": stdout.getvalue(),
            "stderr": stderr.getvalue(),
            "result": result_repr,
            "error": error_payload,
            "exec_count": self.exec_count,
            "duration_ms": duration_ms,
        }

    def _execute_with_expression_result(self, code: str) -> str | None:
        tree = ast.parse(code, mode="exec")
        if not tree.body:
            return None

        # Mirror notebook UX: if the final statement is an expression,
        # evaluate and surface its repr as `Out`.
        if isinstance(tree.body[-1], ast.Expr):
            prefix = tree.body[:-1]
            expr = tree.body[-1].value

            if prefix:
                prefix_module = ast.Module(body=prefix, type_ignores=[])
                exec(
                    compile(prefix_module, "<neurohid-cell>", "exec"),
                    self.globals,
                    self.globals,
                )

            value = eval(  # noqa: S307 - deliberate notebook evaluation surface
                compile(ast.Expression(expr), "<neurohid-cell>", "eval"),
                self.globals,
                self.globals,
            )
            if value is None:
                return None
            return repr(value)

        exec(compile(tree, "<neurohid-cell>", "exec"), self.globals, self.globals)
        return None


def _emit(payload: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def _as_request_id(payload: dict[str, Any]) -> int | None:
    value = payload.get("request_id")
    if isinstance(value, int):
        return value
    return None


def run_stdio() -> None:
    kernel = LabKernel()
    _emit({"type": "ready", "protocol": PROTOCOL_VERSION})

    for raw in sys.stdin:
        line = raw.strip()
        if not line:
            continue

        request_id: int | None = None

        try:
            request = json.loads(line)
            if not isinstance(request, dict):
                raise ValueError("request must be a JSON object")
            request_id = _as_request_id(request)

            request_type = request.get("type")
            if request_type == "execute":
                code = request.get("code")
                if not isinstance(code, str):
                    raise ValueError("execute request must include string field 'code'")

                result = kernel.execute(code)
                _emit(
                    {
                        "type": "execute_result",
                        "request_id": request_id,
                        **result,
                    }
                )
                continue

            if request_type == "reset":
                kernel.reset()
                _emit({"type": "reset_result", "request_id": request_id})
                continue

            if request_type == "ping":
                _emit({"type": "pong", "request_id": request_id})
                continue

            if request_type == "shutdown":
                break

            raise ValueError(f"unsupported request type: {request_type!r}")

        except Exception as exc:  # noqa: BLE001 - keep kernel alive for malformed requests
            _emit(
                {
                    "type": "error",
                    "request_id": request_id,
                    "message": str(exc),
                }
            )


def main() -> None:
    run_stdio()


if __name__ == "__main__":
    main()
