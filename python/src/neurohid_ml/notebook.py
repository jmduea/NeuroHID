"""Notebook-friendly NeuroHID helper API.

This module provides a small convenience layer for Jupyter workflows:
- control channel snapshot/commands
- runtime telemetry polling
- bridge reconnect
- profile training/export/staging wrappers
"""

from __future__ import annotations

import subprocess
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterator

from neurohid_ml.control import NeuroHidControlClient, NotebookError
from neurohid_ml.ipc_constants import (
    CANONICAL_IPC_MODE,
    CANONICAL_LOCAL_ENDPOINT,
)
from neurohid_ml.telemetry import NeuroHidTelemetryClient


@dataclass(slots=True)
class NeuroHidNotebook:
    """Ergonomic API surface for Jupyter notebooks."""

    ipc_mode: str = CANONICAL_IPC_MODE
    ipc_endpoint: str = CANONICAL_LOCAL_ENDPOINT
    service_bin: str = "neurohid-service"
    auto_start_service: bool = True
    service_launch_command: str | None = None
    service_start_wait_secs: float = 1.0
    connect_timeout_secs: float = 1.5
    read_timeout_secs: float = 1.5
    connect_retries: int = 1
    ml_connect_timeout_secs: float = 1.5
    ml_read_timeout_secs: float = 0.2
    _control: NeuroHidControlClient = field(init=False, repr=False)

    def __post_init__(self) -> None:
        self._control = NeuroHidControlClient(
            ipc_mode=self.ipc_mode,
            ipc_endpoint=self.ipc_endpoint,
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

    def daemon_start(self) -> dict[str, Any]:
        return self._control.daemon_start()

    def daemon_stop(self) -> dict[str, Any]:
        return self._control.daemon_stop()

    def daemon_status(self) -> dict[str, Any]:
        return self._control.daemon_status()

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
        return self._control.subscribe_events(
            max_messages=max_messages,
            families=families,
            resume_from_seq=resume_from_seq,
            sample_every=sample_every,
            max_duration_ms=max_duration_ms,
            snapshot_interval_ms=snapshot_interval_ms,
            prefer_stream=prefer_stream,
        )

    def subscribe_observations(
        self,
        *,
        max_messages: int | None = None,
        resume_from_seq: int | None = None,
        sample_every: int = 1,
        max_duration_ms: int | None = None,
    ) -> Iterator[dict[str, Any]]:
        return self.subscribe_events(
            max_messages=max_messages,
            families=["observation_frame"],
            resume_from_seq=resume_from_seq,
            sample_every=sample_every,
            max_duration_ms=max_duration_ms,
        )

    def subscribe_events_with_reconnect(
        self,
        *,
        max_messages: int | None = None,
        families: list[str] | None = None,
        resume_from_seq: int | None = None,
        sample_every: int = 1,
        max_duration_ms: int | None = None,
        snapshot_interval_ms: int = 1_000,
        reconnect_attempts: int = 3,
        reconnect_backoff_secs: float = 0.3,
    ) -> Iterator[dict[str, Any]]:
        emitted = 0
        attempts = 0
        resume_cursor = resume_from_seq
        while max_messages is None or emitted < max_messages:
            remaining = None if max_messages is None else max_messages - emitted
            try:
                for event in self.subscribe_events(
                    max_messages=remaining,
                    families=families,
                    resume_from_seq=resume_cursor,
                    sample_every=sample_every,
                    max_duration_ms=max_duration_ms,
                    snapshot_interval_ms=snapshot_interval_ms,
                ):
                    attempts = 0
                    emitted += 1
                    seq = event.get("_seq")
                    if isinstance(seq, int):
                        resume_cursor = seq
                    yield event
                    if max_messages is not None and emitted >= max_messages:
                        return
                return
            except NotebookError:
                attempts += 1
                if attempts > max(reconnect_attempts, 0):
                    raise
                time.sleep(max(reconnect_backoff_secs, 0.0))

    def observation_to_numpy(self, event_payload: dict[str, Any]) -> Any:
        return self._control.observation_to_numpy(event_payload)

    def events_to_dataframe(self, events: list[dict[str, Any]]) -> Any:
        return self._control.events_to_dataframe(events)

    def telemetry_client(self) -> NeuroHidTelemetryClient:
        return NeuroHidTelemetryClient(
            ipc_mode=self.ipc_mode,
            ipc_endpoint=self.ipc_endpoint,
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
