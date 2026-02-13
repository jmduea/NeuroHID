"""Control-plane client for NeuroHID service endpoints."""

from __future__ import annotations

import json
import os
import socket
import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


class NotebookError(RuntimeError):
    """Raised when a notebook/control convenience operation fails."""


@dataclass(slots=True)
class NeuroHidControlClient:
    """Synchronous control client for NeuroHID TCP or named-pipe endpoints."""

    control_host: str = "127.0.0.1"
    control_port: int = 47385
    control_transport: str = "tcp"
    control_pipe_name: str = r"\\.\pipe\neurohid.control.v2"
    service_bin: str = "neurohid-service"
    auto_start_service: bool = True
    service_launch_command: str | None = None
    service_start_wait_secs: float = 1.0
    connect_timeout_secs: float = 1.5
    read_timeout_secs: float = 1.5
    connect_retries: int = 1
    _service_process: subprocess.Popen[str] | None = field(
        default=None,
        init=False,
        repr=False,
    )

    def snapshot(self) -> dict[str, Any]:
        response = self.send_command({"type": "snapshot"})
        payload = response.get("payload", {})
        snapshot = payload.get("snapshot")
        if not isinstance(snapshot, dict):
            raise NotebookError(f"invalid snapshot response: {response}")
        return snapshot

    def trainer_snapshot(self) -> dict[str, Any]:
        response = self.send_command({"type": "trainer_snapshot"})
        payload = response.get("payload", {})
        snapshot = payload.get("snapshot")
        if not isinstance(snapshot, dict):
            raise NotebookError(f"invalid trainer snapshot response: {response}")
        return snapshot

    def set_output_enabled(self, enabled: bool) -> dict[str, Any]:
        return self.send_command(
            {"type": "set_output_enabled", "enabled": bool(enabled)}
        )

    def set_learning_enabled(self, enabled: bool) -> dict[str, Any]:
        return self.send_command(
            {"type": "set_learning_enabled", "enabled": bool(enabled)}
        )

    def set_fallback_policy(self, policy: dict[str, Any]) -> dict[str, Any]:
        if not isinstance(policy, dict):
            raise NotebookError("fallback policy must be a JSON object")
        return self.send_command({"type": "set_fallback_policy", "policy": policy})

    def reconnect_bridge(self) -> dict[str, Any]:
        return self.send_command({"type": "ml_bridge_reconnect"})

    def reload_model(self) -> dict[str, Any]:
        return self.send_command({"type": "reload_model"})

    def promote_candidate_model(self) -> dict[str, Any]:
        return self.send_command({"type": "promote_candidate_model"})

    def rescan_streams(self) -> dict[str, Any]:
        return self.send_command({"type": "rescan_streams"})

    def connect_stream(self, stream_id: str) -> dict[str, Any]:
        return self.send_command(
            {"type": "connect_stream", "stream_id": str(stream_id)}
        )

    def disconnect_stream(self, stream_id: str) -> dict[str, Any]:
        return self.send_command(
            {"type": "disconnect_stream", "stream_id": str(stream_id)}
        )

    def ensure_connected_stream(self, *, rescan: bool = True) -> str | None:
        if rescan:
            self.rescan_streams()

        snapshot = self.snapshot()
        discovered_streams = snapshot.get("discovered_streams", [])
        if not isinstance(discovered_streams, list):
            raise NotebookError("invalid snapshot: discovered_streams must be a list")

        for stream in discovered_streams:
            if isinstance(stream, dict) and stream.get("connected"):
                stream_id = stream.get("id")
                if isinstance(stream_id, str) and stream_id:
                    return stream_id

        for stream in discovered_streams:
            if not isinstance(stream, dict):
                continue
            if not _is_eligible_eeg_stream(stream):
                continue
            stream_id = stream.get("id")
            if not isinstance(stream_id, str) or not stream_id:
                continue
            self.connect_stream(stream_id)
            return stream_id

        return None

    def send_command(self, command: dict[str, Any]) -> dict[str, Any]:
        request = {
            "request_id": None,
            "command": command,
        }

        payload = json.dumps(request) + "\n"
        data = self._request_endpoint(payload)

        if not data:
            raise NotebookError("empty response from NeuroHID control endpoint")

        response = json.loads(data)
        response_payload = response.get("payload", {})
        if response_payload.get("type") == "error":
            raise NotebookError(
                response_payload.get("message", "unknown control error")
            )
        return response

    def endpoint_label(self) -> str:
        transport = self.control_transport.strip().lower()
        if transport == "pipe":
            return self.control_pipe_name
        return f"{self.control_host}:{self.control_port}"

    def _request_endpoint(self, payload: str) -> str:
        first_error, data = self._try_configured_endpoint(payload)
        if data is not None:
            return data

        service_start_error: str | None = None
        service_start_attempted = False
        if self.auto_start_service:
            service_start_attempted = True
            try:
                started = self._start_service_background()
            except NotebookError as error:
                started = False
                service_start_error = str(error)

            if started:
                time.sleep(max(self.service_start_wait_secs, 0.0))
                first_error, data = self._try_configured_endpoint(payload)
                if data is not None:
                    return data

        error_suffix = f" ({first_error})" if first_error is not None else ""
        startup_suffix = ""
        if service_start_error is not None:
            startup_suffix = f" Auto-start failed: {service_start_error}."
        elif service_start_attempted:
            startup_suffix = (
                " Auto-start was attempted; if this is the first run and "
                "`cargo run` is building in the background, wait 10-30 seconds and retry."
            )

        raise NotebookError(
            "unable to reach NeuroHID control endpoint at "
            f"{self.endpoint_label()}{error_suffix}. "
            "Ensure `neurohid` or `neurohid-service --foreground` is running, "
            "or pass an explicit endpoint via "
            "NeuroHidControlClient(control_transport=..., "
            "control_port/control_pipe_name=...)."
            f"{startup_suffix}"
        )

    def _try_configured_endpoint(
        self, payload: str
    ) -> tuple[Exception | None, str | None]:
        first_error: Exception | None = None
        transport = self.control_transport.strip().lower()

        if transport == "pipe":
            if os.name != "nt":
                raise NotebookError(
                    "control_transport='pipe' is only supported on Windows"
                )
            try:
                data = _request_named_pipe(payload, self.control_pipe_name)
                return first_error, data
            except OSError as error:
                if first_error is None:
                    first_error = error
                return first_error, None

        try:
            data = self._request_once(payload, self.control_port)
            return first_error, data
        except (TimeoutError, OSError) as error:
            if first_error is None:
                first_error = error
            return first_error, None

    def _request_once(self, payload: str, port: int) -> str:
        retries = max(self.connect_retries, 0)
        attempt = 0
        last_error: Exception | None = None
        while attempt <= retries:
            try:
                with socket.create_connection(
                    (self.control_host, port),
                    timeout=self.connect_timeout_secs,
                ) as conn:
                    conn.sendall(payload.encode("utf-8"))
                    conn.settimeout(self.read_timeout_secs)
                    return _read_line(conn)
            except (TimeoutError, OSError) as error:
                last_error = error
                attempt += 1
                if attempt <= retries:
                    time.sleep(0.15)

        assert last_error is not None
        raise last_error

    def _start_service_background(self) -> bool:
        if self._service_process is not None and self._service_process.poll() is None:
            return True

        commands = self._service_launch_commands()
        errors: list[str] = []
        for command, cwd in commands:
            try:
                process = _spawn_background_process(command, cwd=cwd)
                time.sleep(0.35)
                exit_code = process.poll()
                if exit_code is not None:
                    errors.append(
                        " ".join(command)
                        + f" (exited immediately with code {exit_code})"
                    )
                    continue
                self._service_process = process
                return True
            except OSError as error:
                errors.append(f"{' '.join(command)} ({error})")

        raise NotebookError(
            "failed to auto-start neurohid service: " + "; ".join(errors)
        )

    def _service_launch_commands(self) -> list[tuple[list[str], str | None]]:
        if self.service_launch_command:
            return [(["cmd", "/D", "/S", "/C", self.service_launch_command], None)]

        repo_root_path = Path(__file__).resolve().parents[3]
        binary_name = "neurohid-service.exe" if os.name == "nt" else "neurohid-service"
        built_binary_path = repo_root_path / "target" / "debug" / binary_name

        commands: list[tuple[list[str], str | None]] = [
            (
                [
                    self.service_bin,
                    "--foreground",
                    "--control-port",
                    str(self.control_port),
                ],
                None,
            )
        ]

        if built_binary_path.exists():
            commands.append(
                (
                    [
                        str(built_binary_path),
                        "--foreground",
                        "--control-port",
                        str(self.control_port),
                    ],
                    str(repo_root_path),
                )
            )

        commands.append(
            (
                [
                    "cargo",
                    "run",
                    "--bin",
                    "neurohid-service",
                    "--",
                    "--foreground",
                    "--control-port",
                    str(self.control_port),
                ],
                str(repo_root_path),
            )
        )
        return commands


def _request_named_pipe(payload: str, pipe_name: str) -> str:
    with open(pipe_name, "r+b", buffering=0) as pipe:
        pipe.write(payload.encode("utf-8"))
        return _read_line(pipe)


def _read_line(conn: Any) -> str:
    chunks: list[bytes] = []
    while True:
        if hasattr(conn, "recv"):
            byte = conn.recv(1)
        else:
            byte = conn.read(1)
        if not byte:
            break
        if byte == b"\n":
            break
        chunks.append(byte)
    return b"".join(chunks).decode("utf-8", errors="replace").strip()


def _is_eligible_eeg_stream(stream: dict[str, Any]) -> bool:
    stream_type = stream.get("stream_type")
    if not isinstance(stream_type, str):
        return False
    if not stream_type.startswith("EEG/"):
        return False

    channel_count = stream.get("channel_count")
    if not isinstance(channel_count, int) or channel_count <= 0:
        return False

    sample_rate = stream.get("sample_rate")
    if isinstance(sample_rate, (float, int)) and sample_rate > 0:
        return True

    return stream_type in {"EEG/EmotivEEG", "EEG"}


def _spawn_background_process(
    command: list[str],
    *,
    cwd: str | None,
) -> subprocess.Popen[str]:
    kwargs: dict[str, Any] = {
        "stdin": subprocess.DEVNULL,
        "stdout": subprocess.DEVNULL,
        "stderr": subprocess.DEVNULL,
        "text": True,
        "cwd": cwd,
    }

    if os.name == "nt":
        kwargs["creationflags"] = (
            subprocess.CREATE_NEW_PROCESS_GROUP
            | subprocess.DETACHED_PROCESS
            | subprocess.CREATE_NO_WINDOW
        )

    return subprocess.Popen(command, **kwargs)
