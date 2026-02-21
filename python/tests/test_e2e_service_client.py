"""E2E test: spawn neurohid-service, connect with Python control client, snapshot and control.

Satisfies TEST-04: at least one valuable E2E path (service + Python client).
Uses condition-based wait for service readiness (no sleep-only). Ephemeral port to avoid
conflicts. Runs with: uv run --project python pytest python/tests/test_e2e_service_client.py -v
"""

from __future__ import annotations

import os
import socket
import subprocess
import sys
import time
from pathlib import Path

import pytest

# Ensure python/src on path when running from repo root
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from neurohid_ml.control import NeuroHidControlClient


def _allocate_ephemeral_port() -> int:
    """Bind to 127.0.0.1:0 and return the allocated port."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def _wait_for_port(host: str, port: int, deadline_secs: float = 15.0) -> None:
    """Poll until TCP connect succeeds or deadline. Condition-based wait (no sleep-only)."""
    deadline = time.monotonic() + deadline_secs
    last_err: Exception | None = None
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=1.0):
                return
        except (OSError, TimeoutError) as e:
            last_err = e
            time.sleep(0.1)
    msg = f"port {host}:{port} did not become reachable within {deadline_secs}s"
    if last_err is not None:
        msg += f"; last error: {last_err}"
    raise AssertionError(msg)


def _wait_for_snapshot(client: NeuroHidControlClient, deadline_secs: float = 20.0) -> dict:
    """Poll until snapshot() returns successfully or deadline. Condition-based wait."""
    deadline = time.monotonic() + deadline_secs
    last_err: Exception | None = None
    while time.monotonic() < deadline:
        try:
            snap = client.snapshot()
            if isinstance(snap, dict) and "running" in snap:
                return snap
        except Exception as e:
            last_err = e
        time.sleep(0.25)
    msg = f"snapshot did not succeed within {deadline_secs}s"
    if last_err is not None:
        msg += f"; last error: {last_err}"
    raise AssertionError(msg)


def _find_service_binary() -> Path | None:
    """Return path to neurohid-service binary (target/debug or target/release), or None."""
    repo_root = Path(__file__).resolve().parents[2]
    name = "neurohid-service.exe" if os.name == "nt" else "neurohid-service"
    for sub in ("debug", "release"):
        candidate = repo_root / "target" / sub / name
        if candidate.exists():
            return candidate
    return None


@pytest.fixture(scope="module")
def service_binary():
    """Resolve neurohid-service binary; skip test if not built."""
    path = _find_service_binary()
    if path is None:
        pytest.skip(
            "neurohid-service binary not found; run cargo build -p neurohid --bin neurohid-service"
        )
    return path


@pytest.fixture
def ephemeral_port():
    """Per-test ephemeral port to avoid conflicts."""
    return _allocate_ephemeral_port()


@pytest.mark.skipif(
    sys.platform == "win32",
    reason="E2E service+client runs in CI on Linux; Windows timing/env can be fragile",
)
def test_e2e_spawn_service_snapshot_and_control(
    service_binary: Path,
    ephemeral_port: int,
) -> None:
    """Spawn neurohid-service on ephemeral port, connect with Python client, snapshot and control."""
    env = os.environ.copy()
    # Isolate test: avoid shared state; service may write to default data dir
    cwd = str(Path(__file__).resolve().parents[2])

    proc = subprocess.Popen(
        [
            str(service_binary),
            "--foreground",
            "--control-port",
            str(ephemeral_port),
        ],
        cwd=cwd,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
    )
    try:
        _wait_for_port("127.0.0.1", ephemeral_port, deadline_secs=15.0)

        client = NeuroHidControlClient(
            ipc_mode="tcp_loopback",
            ipc_endpoint=f"127.0.0.1:{ephemeral_port}",
            auto_start_service=False,
            connect_timeout_secs=2.0,
            read_timeout_secs=5.0,
        )
        snapshot = _wait_for_snapshot(client, deadline_secs=20.0)

        assert isinstance(snapshot, dict), "snapshot must be a dict"
        assert "running" in snapshot, "snapshot must include running"
        assert snapshot["running"] is True, "runtime should report running"
        assert "discovered_streams" in snapshot, "snapshot must include discovered_streams"
        assert isinstance(snapshot["discovered_streams"], list), "discovered_streams must be a list"

        # Optional control: set_output_enabled roundtrip
        client.set_output_enabled(False)
        snap2 = client.snapshot()
        assert snap2.get("output_enabled") is False
        client.set_output_enabled(True)
        snap3 = client.snapshot()
        assert snap3.get("output_enabled") is True
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=2)
