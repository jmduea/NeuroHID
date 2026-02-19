"""Control-plane client for NeuroHID service endpoints."""

from __future__ import annotations

import json
import os
import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterator

from neurohid_ml.ipc import NeuroHidIpcClient, events_to_dataframe, observation_to_numpy
from neurohid_ml.ipc_constants import (
    CANONICAL_IPC_MODE,
    CANONICAL_LOCAL_ENDPOINT,
    parse_tcp_endpoint as _parse_tcp_endpoint,
)


class NotebookError(RuntimeError):
    """Raised when a notebook/control convenience operation fails."""


@dataclass(slots=True)
class NeuroHidControlClient:
    """Synchronous control client for NeuroHID IPC endpoints."""

    ipc_mode: str = CANONICAL_IPC_MODE
    ipc_endpoint: str = CANONICAL_LOCAL_ENDPOINT
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
        return self.send_command({"type": "set_output_enabled", "enabled": bool(enabled)})

    def set_learning_enabled(self, enabled: bool) -> dict[str, Any]:
        return self.send_command({"type": "set_learning_enabled", "enabled": bool(enabled)})

    def set_fallback_policy(self, policy: dict[str, Any]) -> dict[str, Any]:
        if not isinstance(policy, dict):
            raise NotebookError("fallback policy must be a JSON object")
        return self.send_command({"type": "set_fallback_policy", "policy": policy})

    def reconnect_bridge(self) -> dict[str, Any]:
        return self.send_command({"type": "ml_bridge_reconnect"})

    def daemon_start(self) -> dict[str, Any]:
        return self._run_daemon_command("start")

    def daemon_stop(self) -> dict[str, Any]:
        return self._run_daemon_command("stop")

    def daemon_status(self) -> dict[str, Any]:
        return self._run_daemon_command("status")

    def reload_model(self) -> dict[str, Any]:
        return self.send_command({"type": "reload_model"})

    def promote_candidate_model(self) -> dict[str, Any]:
        return self.send_command({"type": "promote_candidate_model"})

    def rescan_streams(self) -> dict[str, Any]:
        return self.send_command({"type": "rescan_streams"})

    def connect_stream(self, stream_id: str) -> dict[str, Any]:
        return self.send_command({"type": "connect_stream", "stream_id": str(stream_id)})

    def disconnect_stream(self, stream_id: str) -> dict[str, Any]:
        return self.send_command({"type": "disconnect_stream", "stream_id": str(stream_id)})

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
        response_payload = self._request_endpoint(command)
        if isinstance(response_payload, str):
            try:
                decoded = json.loads(response_payload)
            except json.JSONDecodeError as error:
                raise NotebookError(f"invalid control response payload: {error}") from error
            if isinstance(decoded, dict) and isinstance(decoded.get("payload"), dict):
                response_payload = decoded.get("payload", {})
            elif isinstance(decoded, dict):
                response_payload = decoded
            else:
                raise NotebookError("invalid control response payload: expected object")
        response = {"payload": response_payload}
        response_payload = response.get("payload", {})
        if response_payload.get("type") == "error":
            raise NotebookError(response_payload.get("message", "unknown control error"))
        return response

    def endpoint_label(self) -> str:
        return self._build_ipc_client().endpoint_label()

    def subscribe_events(
        self,
        *,
        max_messages: int | None = None,
        families: list[str] | None = None,
        resume_from_seq: int | None = None,
        sample_every: int = 1,
        max_duration_ms: int | None = None,
        snapshot_interval_ms: int = 1_000,
        prefer_stream: bool = True,
    ) -> Iterator[dict[str, Any]]:
        ipc = self._build_ipc_client()
        return ipc.iter_runtime_events(
            max_messages=max_messages,
            families=families,
            resume_from_seq=resume_from_seq,
            sample_every=sample_every,
            max_duration_ms=max_duration_ms,
            snapshot_interval_ms=snapshot_interval_ms,
            prefer_stream=prefer_stream,
        )

    def observation_to_numpy(self, event_payload: dict[str, Any]) -> Any:
        return observation_to_numpy(event_payload)

    def events_to_dataframe(self, events: list[dict[str, Any]]) -> Any:
        return events_to_dataframe(events)

    def _request_endpoint(self, command: dict[str, Any]) -> dict[str, Any]:
        first_error, response = self._try_configured_endpoint(command)
        if response is not None:
            return response

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
                first_error, response = self._try_configured_endpoint(command)
                if response is not None:
                    return response

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
            "NeuroHidControlClient(ipc_mode=..., ipc_endpoint=...)."
            f"{startup_suffix}"
        )

    def _try_configured_endpoint(
        self, command: dict[str, Any]
    ) -> tuple[Exception | None, dict[str, Any] | None]:
        first_error: Exception | None = None
        try:
            ipc = self._build_ipc_client()
            response = ipc.send_control_command(command)
            return first_error, response
        except Exception as error:  # noqa: BLE001
            if first_error is None:
                first_error = error
            return first_error, None

    def _build_ipc_client(self) -> NeuroHidIpcClient:
        return NeuroHidIpcClient(
            ipc_mode=self.ipc_mode,
            ipc_endpoint=self.ipc_endpoint,
            connect_timeout_secs=self.connect_timeout_secs,
            read_timeout_secs=self.read_timeout_secs,
            connect_retries=self.connect_retries,
        )

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
                        " ".join(command) + f" (exited immediately with code {exit_code})"
                    )
                    continue
                self._service_process = process
                return True
            except OSError as error:
                errors.append(f"{' '.join(command)} ({error})")

        raise NotebookError("failed to auto-start neurohid service: " + "; ".join(errors))

    def _service_launch_commands(self) -> list[tuple[list[str], str | None]]:
        if self.service_launch_command:
            return [(["cmd", "/D", "/S", "/C", self.service_launch_command], None)]

        repo_root_path = Path(__file__).resolve().parents[3]
        binary_name = "neurohid-service.exe" if os.name == "nt" else "neurohid-service"
        built_binary_path = repo_root_path / "target" / "debug" / binary_name
        port_override = self._daemon_control_port_override()

        foreground_cmd = [self.service_bin, "--foreground"]
        if port_override is not None:
            foreground_cmd.extend(["--control-port", str(port_override)])

        commands: list[tuple[list[str], str | None]] = [(foreground_cmd, None)]

        if built_binary_path.exists():
            built_cmd = [str(built_binary_path), "--foreground"]
            if port_override is not None:
                built_cmd.extend(["--control-port", str(port_override)])
            commands.append(
                (
                    built_cmd,
                    str(repo_root_path),
                )
            )

        cargo_cmd = [
            "cargo",
            "run",
            "--bin",
            "neurohid-service",
            "--",
            "--foreground",
        ]
        if port_override is not None:
            cargo_cmd.extend(["--control-port", str(port_override)])

        commands.append(
            (
                cargo_cmd,
                str(repo_root_path),
            )
        )
        return commands

    def _run_daemon_command(self, command: str) -> dict[str, Any]:
        cmd = [self.service_bin, "daemon", command]
        port_override = self._daemon_control_port_override()
        if port_override is not None:
            cmd.extend(["--control-port", str(port_override)])
        completed = subprocess.run(cmd, check=False, text=True, capture_output=True)
        if completed.returncode != 0:
            details = ""
            if completed.stdout:
                details += f"\nstdout:\n{completed.stdout}"
            if completed.stderr:
                details += f"\nstderr:\n{completed.stderr}"
            raise NotebookError(
                f"daemon command failed ({completed.returncode}): {' '.join(cmd)}{details}"
            )
        return {
            "payload": {
                "type": "daemon_status",
                "command": command,
                "stdout": completed.stdout.strip(),
                "stderr": completed.stderr.strip(),
            }
        }

    def _daemon_control_port_override(self) -> int | None:
        ipc = self._build_ipc_client()
        if ipc.ipc_mode.strip().lower() != "tcp_loopback":
            return None
        _, port = _parse_tcp_endpoint(self.ipc_endpoint)
        return port


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
        create_new_process_group = getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
        detached_process = getattr(subprocess, "DETACHED_PROCESS", 0)
        create_no_window = getattr(subprocess, "CREATE_NO_WINDOW", 0)
        kwargs["creationflags"] = create_new_process_group | detached_process | create_no_window

    return subprocess.Popen(command, **kwargs)
