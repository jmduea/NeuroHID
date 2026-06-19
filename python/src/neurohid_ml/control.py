"""Control-plane client for an in-process NeuroHID runtime.

Wraps a ``neurohid.RuntimeHandle`` obtained from ``RuntimeBuilder.start()``
and exposes synchronous convenience methods that mirror the old IPC control
client API.
"""

from __future__ import annotations

import json
from typing import Any

from neurohid_ml.ipc import events_to_dataframe, observation_to_numpy


class NotebookError(RuntimeError):
    """Raised when a notebook/control convenience operation fails."""


class NeuroHidControlClient:
    """Synchronous control client backed by an in-process ``RuntimeHandle``.

    Parameters
    ----------
    runtime:
        A ``neurohid.RuntimeHandle`` returned by ``await RuntimeBuilder(config).start()``.
    """

    def __init__(self, runtime: Any) -> None:
        self._runtime = runtime

    # -- Snapshots -----------------------------------------------------------

    def snapshot(self) -> dict[str, Any]:
        """Return a point-in-time runtime state snapshot as a dict."""
        return self._runtime.snapshot().to_dict()

    def trainer_snapshot(self) -> dict[str, Any]:
        """Return the trainer bridge status snapshot as a dict."""
        return self._runtime.trainer_snapshot().to_dict()

    # -- Simple commands (synchronous, fire-and-forget) ----------------------

    def set_output_enabled(self, enabled: bool) -> dict[str, Any]:
        self._runtime.command("toggle_output", enabled=bool(enabled))
        return {"status": "ok"}

    def set_learning_enabled(self, enabled: bool) -> dict[str, Any]:
        self._runtime.command("set_learning_enabled", enabled=bool(enabled))
        return {"status": "ok"}

    def reconnect_bridge(self) -> dict[str, Any]:
        self._runtime.command("ml_bridge_reconnect")
        return {"status": "ok"}

    def reload_model(self) -> dict[str, Any]:
        self._runtime.command("reload_model")
        return {"status": "ok"}

    def promote_candidate_model(self) -> dict[str, Any]:
        self._runtime.command("promote_candidate_model")
        return {"status": "ok"}

    def rescan_streams(self) -> dict[str, Any]:
        self._runtime.command("rescan_streams")
        return {"status": "ok"}

    def connect_stream(self, stream_id: str) -> dict[str, Any]:
        self._runtime.command("connect_stream", stream_id=str(stream_id))
        return {"status": "ok"}

    def disconnect_stream(self, stream_id: str) -> dict[str, Any]:
        self._runtime.command("disconnect_stream", stream_id=str(stream_id))
        return {"status": "ok"}

    # -- Control requests (synchronous, round-trip via dispatch_control_sync) -

    def set_fallback_policy(self, policy: dict[str, Any]) -> dict[str, Any]:
        if not isinstance(policy, dict):
            raise NotebookError("fallback policy must be a JSON object")
        return self.dispatch_control({"type": "set_fallback_policy", "policy": policy})

    def dispatch_control(self, request: dict[str, Any]) -> dict[str, Any]:
        """Send an arbitrary control request and return the response dict.

        Uses the synchronous ``dispatch_control_sync`` method on the native
        ``RuntimeHandle``. Callers may provide either the Rust ``ControlRequest``
        shape or a bare ``ControlCommand``; bare commands are wrapped here.
        """
        result_json = self._runtime.dispatch_control_sync(json.dumps(_control_request(request)))
        return json.loads(result_json)

    # -- Stream discovery helpers --------------------------------------------

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

    # -- In-process stream subscriptions (async iterators) -------------------

    def subscribe_samples(self):
        """Return an async iterator of ``Sample`` objects."""
        return self._runtime.subscribe_samples()

    def subscribe_features(self):
        """Return an async iterator of ``FeatureVector`` objects."""
        return self._runtime.subscribe_features()

    def subscribe_actions(self):
        """Return an async iterator of ``Action`` objects."""
        return self._runtime.subscribe_actions()

    def subscribe_markers(self):
        """Return an async iterator of ``StreamMarker`` objects."""
        return self._runtime.subscribe_markers()

    def subscribe_events(self):
        """Return an async iterator of ``RuntimeEvent`` objects."""
        return self._runtime.subscribe_events()

    # -- Runtime lifecycle ---------------------------------------------------

    def is_alive(self) -> bool:
        return self._runtime.is_alive()

    # -- Data helpers --------------------------------------------------------

    def observation_to_numpy(self, event_payload: dict[str, Any]) -> Any:
        return observation_to_numpy(event_payload)

    def events_to_dataframe(self, events: list[dict[str, Any]]) -> Any:
        return events_to_dataframe(events)


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


def _control_request(request: dict[str, Any]) -> dict[str, Any]:
    """Return a Rust ``ControlRequest`` JSON object for a command payload."""
    if not isinstance(request, dict):
        raise NotebookError("control request must be a JSON object")

    if "command" in request:
        command = request["command"]
        if not isinstance(command, dict):
            raise NotebookError("control request command must be a JSON object")
        return {
            "request_id": request.get("request_id"),
            "command": command,
        }

    return {
        "request_id": request.get("request_id"),
        "command": {key: value for key, value in request.items() if key != "request_id"},
    }
