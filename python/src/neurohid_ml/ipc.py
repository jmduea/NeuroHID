"""Data-transform utilities for NeuroHID runtime events.

The socket-based ``NeuroHidIpcClient`` that used to live here has been replaced
by in-process bindings in the ``neurohid`` native extension module.  Use
``neurohid.RuntimeBuilder`` / ``neurohid.RuntimeHandle`` for runtime
interaction.

The observation/event helper functions are retained for notebook convenience.
"""

from __future__ import annotations

from typing import Any, Sequence


def observation_to_vector(event_payload: dict[str, Any]) -> list[float]:
    """Flatten one ``observation_frame`` payload into a numeric vector."""
    observation = event_payload.get("observation")
    if not isinstance(observation, dict):
        raise RuntimeError("event payload does not include `observation` object")

    vector: list[float] = []
    signal = observation.get("signal_features")
    if isinstance(signal, dict):
        values = signal.get("values")
        if isinstance(values, list):
            vector.extend(float(v) for v in values)

    cursor = observation.get("cursor")
    if isinstance(cursor, dict):
        vector.extend(
            [
                float(cursor.get("x", 0.0)),
                float(cursor.get("y", 0.0)),
                float(cursor.get("velocity_x", 0.0)),
                float(cursor.get("velocity_y", 0.0)),
                1.0 if bool(cursor.get("button_held", False)) else 0.0,
            ]
        )

    screen = observation.get("screen")
    if isinstance(screen, dict):
        width = float(screen.get("width", 1.0) or 1.0)
        height = float(screen.get("height", 1.0) or 1.0)
        monitors = float(screen.get("monitor_count", 1.0) or 1.0)
        aspect = width / max(height, 1.0)
        vector.extend([aspect, monitors])

    return vector


def observation_to_numpy(event_payload: dict[str, Any]) -> Any:
    """Return NumPy array view of one observation event payload."""
    try:
        import numpy as np
    except Exception as error:  # pragma: no cover - optional dependency
        raise RuntimeError("NumPy is required for observation_to_numpy()") from error
    return np.asarray(observation_to_vector(event_payload), dtype=np.float32)


def events_to_dataframe(events: Sequence[dict[str, Any]]) -> Any:
    """Convert a sequence of runtime event payloads to a pandas DataFrame."""
    try:
        import pandas as pd  # type: ignore[import-untyped]
    except Exception as error:  # pragma: no cover - optional dependency
        raise RuntimeError("pandas is required for events_to_dataframe()") from error

    rows: list[dict[str, Any]] = []
    for payload in events:
        if not isinstance(payload, dict):
            continue
        row = {"event_type": payload.get("type")}
        if payload.get("type") == "observation_frame":
            row["observation_vector"] = observation_to_vector(payload)
        observation = payload.get("observation")
        if isinstance(observation, dict):
            row["timestamp"] = observation.get("timestamp")
            signal = observation.get("signal_features")
            if isinstance(signal, dict):
                values = signal.get("values")
                if isinstance(values, list):
                    row["feature_dim"] = len(values)
        rows.append(row)
    return pd.DataFrame(rows)
