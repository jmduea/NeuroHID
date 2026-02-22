"""Runtime event stream client backed by an in-process ``RuntimeHandle``.

The socket-based transport has been replaced by the in-process ``neurohid``
native extension.  This module exposes a thin async wrapper around the
``RuntimeHandle.subscribe_events()`` async iterator.
"""

from __future__ import annotations

from typing import Any

from neurohid_ml.control import NotebookError


class NeuroHidTelemetryClient:
    """Async runtime-events reader backed by an in-process ``RuntimeHandle``.

    Parameters
    ----------
    runtime:
        A ``neurohid.RuntimeHandle`` returned by ``await RuntimeBuilder(config).start()``.
    """

    def __init__(self, runtime: Any) -> None:
        self._runtime = runtime

    def subscribe_events(self):
        """Return an async iterator of ``RuntimeEvent`` objects."""
        return self._runtime.subscribe_events()

    def is_alive(self) -> bool:
        return self._runtime.is_alive()
