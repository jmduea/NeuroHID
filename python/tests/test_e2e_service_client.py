"""E2E test placeholder.

The original E2E test spawned a separate neurohid-service process and connected
via socket IPC.  The Python bindings now use in-process ``RuntimeHandle`` from
the ``neurohid`` native module, so the spawn-and-connect pattern no longer
applies.

In-process E2E tests should start a ``RuntimeBuilder`` and exercise the
``NeuroHidControlClient`` directly. This file is kept as a placeholder for
future in-process E2E coverage.
"""

from __future__ import annotations

import pytest


@pytest.mark.skip(reason="in-process bindings replace the socket-based E2E path")
def test_e2e_placeholder() -> None:
    pass
