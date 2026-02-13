"""Notebook-friendly NeuroHID helper API.

This module provides a small convenience layer for Jupyter workflows:
- control channel snapshot/commands
- runtime telemetry polling
- bridge reconnect
- profile training/export/staging wrappers
"""

from __future__ import annotations

import os
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterator

from neurohid_ml.control import NeuroHidControlClient, NotebookError
from neurohid_ml.telemetry import NeuroHidTelemetryClient


@dataclass(slots=True)
class NeuroHidNotebook:
    """Ergonomic API surface for Jupyter notebooks."""

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
    ml_transport: str = "named_pipe" if os.name == "nt" else "tcp_loopback"
    ml_host: str = "127.0.0.1"
    ml_port: int = 47384
    ml_pipe_name: str = r"\\.\pipe\neurohid.ml.v2"
    ml_connect_timeout_secs: float = 1.5
    ml_read_timeout_secs: float = 0.2
    _control: NeuroHidControlClient = field(init=False, repr=False)

    def __post_init__(self) -> None:
        self._control = NeuroHidControlClient(
            control_host=self.control_host,
            control_port=self.control_port,
            control_transport=self.control_transport,
            control_pipe_name=self.control_pipe_name,
            service_bin=self.service_bin,
            auto_start_service=self.auto_start_service,
            service_launch_command=self.service_launch_command,
            service_start_wait_secs=self.service_start_wait_secs,
            connect_timeout_secs=self.connect_timeout_secs,
            read_timeout_secs=self.read_timeout_secs,
            connect_retries=self.connect_retries,
        )

    def snapshot(self) -> dict[str, Any]:
        return self._control.snapshot()

    def trainer_snapshot(self) -> dict[str, Any]:
        return self._control.trainer_snapshot()

    def set_output_enabled(self, enabled: bool) -> dict[str, Any]:
        return self._control.set_output_enabled(enabled)

    def set_learning_enabled(self, enabled: bool) -> dict[str, Any]:
        return self._control.set_learning_enabled(enabled)

    def set_fallback_policy(self, policy: dict[str, Any]) -> dict[str, Any]:
        return self._control.set_fallback_policy(policy)

    def reconnect_bridge(self) -> dict[str, Any]:
        return self._control.reconnect_bridge()

    def reload_model(self) -> dict[str, Any]:
        return self._control.reload_model()

    def promote_candidate_model(self) -> dict[str, Any]:
        return self._control.promote_candidate_model()

    def rescan_streams(self) -> dict[str, Any]:
        return self._control.rescan_streams()

    def connect_stream(self, stream_id: str) -> dict[str, Any]:
        return self._control.connect_stream(stream_id)

    def disconnect_stream(self, stream_id: str) -> dict[str, Any]:
        return self._control.disconnect_stream(stream_id)

    def ensure_connected_stream(self, *, rescan: bool = True) -> str | None:
        return self._control.ensure_connected_stream(rescan=rescan)

    def telemetry_client(self) -> NeuroHidTelemetryClient:
        return NeuroHidTelemetryClient(
            transport=self.ml_transport,
            host=self.ml_host,
            port=self.ml_port,
            pipe_name=self.ml_pipe_name,
            connect_timeout_secs=self.ml_connect_timeout_secs,
            read_timeout_secs=self.ml_read_timeout_secs,
        )

    def recv_telemetry(self) -> dict[str, Any] | None:
        return self.telemetry_client().recv()

    def iter_telemetry(
        self,
        *,
        max_messages: int | None = None,
    ) -> Iterator[dict[str, Any]]:
        return self.telemetry_client().iter_messages(max_messages=max_messages)

    def train_profile_candidate(
        self,
        profile_id: str,
        *,
        work_dir: str | Path | None = None,
        output_dir: str | Path | None = None,
        keep_work_dir: bool = False,
        epochs: int = 10,
        learning_rate: float = 1e-3,
        holdout_ratio: float = 0.2,
        seed: int = 7,
        decode_latency_p95_us: int = 40_000,
        min_samples: int = 64,
    ) -> subprocess.CompletedProcess[str]:
        command = [
            "neurohid-ml",
            "train-profile-candidate",
            "--profile-id",
            profile_id,
            "--service-bin",
            self.service_bin,
            "--epochs",
            str(epochs),
            "--learning-rate",
            str(learning_rate),
            "--holdout-ratio",
            str(holdout_ratio),
            "--seed",
            str(seed),
            "--decode-latency-p95-us",
            str(decode_latency_p95_us),
            "--min-samples",
            str(min_samples),
        ]
        if work_dir is not None:
            command.extend(["--work-dir", str(work_dir)])
        if output_dir is not None:
            command.extend(["--output-dir", str(output_dir)])
        if keep_work_dir:
            command.append("--keep-work-dir")

        return self._run_command(command)

    def export_session_logs(
        self,
        profile_id: str,
        output_dir: str | Path,
    ) -> subprocess.CompletedProcess[str]:
        command = [
            self.service_bin,
            "--profile",
            profile_id,
            "--export-session-logs-dir",
            str(output_dir),
        ]
        return self._run_command(command)

    def import_candidate_dir(
        self,
        profile_id: str,
        candidate_dir: str | Path,
    ) -> subprocess.CompletedProcess[str]:
        command = [
            self.service_bin,
            "--profile",
            profile_id,
            "--import-candidate-dir",
            str(candidate_dir),
        ]
        return self._run_command(command)

    def _run_command(self, command: list[str]) -> subprocess.CompletedProcess[str]:
        completed = subprocess.run(command, check=False, text=True, capture_output=True)
        if completed.returncode != 0:
            details = ""
            if completed.stdout:
                details += f"\nstdout:\n{completed.stdout}"
            if completed.stderr:
                details += f"\nstderr:\n{completed.stderr}"
            raise NotebookError(
                f"command failed ({completed.returncode}): {' '.join(command)}{details}"
            )
        return completed
