"""Notebook-friendly NeuroHID helper API.

This module provides a small convenience layer for Jupyter workflows:
- control channel snapshot/commands
- runtime telemetry polling
- bridge reconnect
- profile training/export/staging wrappers

All communication uses an in-process ``RuntimeHandle`` obtained from the
``neurohid`` native extension.
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from neurohid_ml.control import NeuroHidControlClient, NotebookError
from neurohid_ml.telemetry import NeuroHidTelemetryClient


@dataclass(slots=True)
class NeuroHidNotebook:
    """Ergonomic API surface for Jupyter notebooks.

    Parameters
    ----------
    runtime:
        A ``neurohid.RuntimeHandle`` returned by ``await RuntimeBuilder(config).start()``.
    service_bin:
        Path to the service binary used for subprocess training/export commands.
    """

    runtime: Any
    service_bin: str = "neurohid-service"
    _control: NeuroHidControlClient = field(init=False, repr=False)

    def __post_init__(self) -> None:
        self._control = NeuroHidControlClient(self.runtime)

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

    def subscribe_samples(self):
        """Return an async iterator of ``Sample`` objects."""
        return self._control.subscribe_samples()

    def subscribe_features(self):
        """Return an async iterator of ``FeatureVector`` objects."""
        return self._control.subscribe_features()

    def subscribe_actions(self):
        """Return an async iterator of ``Action`` objects."""
        return self._control.subscribe_actions()

    def subscribe_markers(self):
        """Return an async iterator of ``StreamMarker`` objects."""
        return self._control.subscribe_markers()

    def subscribe_events(self):
        """Return an async iterator of ``RuntimeEvent`` objects."""
        return self._control.subscribe_events()

    def is_alive(self) -> bool:
        return self._control.is_alive()

    # -- Data helpers --------------------------------------------------------

    def observation_to_numpy(self, event_payload: dict[str, Any]) -> Any:
        return self._control.observation_to_numpy(event_payload)

    def events_to_dataframe(self, events: list[dict[str, Any]]) -> Any:
        return self._control.events_to_dataframe(events)

    # -- Telemetry -----------------------------------------------------------

    def telemetry_client(self) -> NeuroHidTelemetryClient:
        return NeuroHidTelemetryClient(self.runtime)

    # -- Subprocess training/export helpers ----------------------------------

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
