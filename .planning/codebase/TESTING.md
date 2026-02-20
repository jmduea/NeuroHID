# Testing Patterns

**Analysis Date:** 2026-02-20

The repository has two test surfaces: **Rust** (unit tests inside crates) and **Python** (unittest-based tests in `python/tests/`). No shared E2E or integration test runner is defined.

---

## Test Framework

### Rust

- **Runner:** `cargo test` (built-in test harness).
- **Assertions:** Standard `assert!`, `assert_eq!`, `assert_relative_eq!` (from workspace dependency `approx = "0.5.1"` for float comparison).
- **Run commands:**
  - `cargo test --workspace` — all crates
  - `cargo test -p <crate-name>` — single crate
  - Per-lane gate: `cargo test --workspace` (see `crates/AGENTS.md`).

### Python

- **Runner:** pytest (optional dev dependency `pytest>=9.0.2`). Tests are written with the **unittest** API (`unittest.TestCase`, `unittest.IsolatedAsyncioTestCase`); pytest discovers and runs them.
- **Assertion library:** unittest (`self.assert*`, `self.assertRaisesRegex`, etc.).
- **Async:** `unittest.IsolatedAsyncioTestCase` for async test methods (e.g. `python/tests/test_bridge.py`).
- **Run commands:**
  - `uv run --project python pytest python/tests -q` — run all tests (see `python/AGENTS.md`)
  - `uv run --project python pytest python/tests -v` — verbose
  - `uv run --project python pytest python/tests -k <expr>` — filter by name
  - Optional: `python -m unittest discover` works due to `if __name__ == "__main__": unittest.main()` in each test file.

---

## Test File Organization

### Rust

- **Location:** Co-located with source. Each file that has tests contains a `#[cfg(test)] mod tests { ... }` block at the bottom (sometimes after a `// ----- Tests -----` style comment).
- **Naming:** Test module is always `mod tests`; individual tests are `#[test] fn test_<behavior>_<scenario>` or `fn <behavior>_<scenario>` (e.g. `fn decision_event_roundtrips_payload`, `fn test_lowpass_passes_dc`).
- **Structure:**
  - `use super::*;` and `use crate::...` for types under test.
  - Helper functions or constants inside `mod tests` (e.g. `sample_snapshot()` in `crates/neurohid-types/src/ipc.rs`, `fn sample_snapshot() -> ControlSnapshot { ... }`).
  - No shared test crate; each crate’s tests live in that crate.

**Example (pattern):**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_lowpass_passes_dc() {
        let mut bq = Biquad::lowpass(10.0, 128.0);
        // ...
        assert_relative_eq!(output, 1.0, epsilon = 0.01);
    }
}
```

Files with substantial test blocks include: `crates/neurohid-types/src/ipc.rs`, `crates/neurohid-signal/src/filter.rs`, `crates/neurohid-storage/src/paths.rs`, `crates/neurohid-sdk/src/lib.rs`, `crates/neurohid-core/src/service.rs`, `crates/neurohid-hub/src/app.rs`, and many others under `crates/`.

### Python

- **Location:** All under `python/tests/`. No tests next to source in `src/neurohid_ml/`.
- **Naming:** `test_<subject>.py` (e.g. `test_bridge.py`, `test_decoder_and_errp.py`, `test_control_client.py`, `test_cli_and_clients.py`, `test_trainer.py`, `test_notebook_helpers.py`, `test_lab_kernel.py`).
- **Structure:**
  - Boilerplate at top: `from __future__ import annotations`, then stdlib imports, then `sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))`, then `importlib.import_module("neurohid_ml.<module>")` assigned to a private name (e.g. `_bridge`, `_decoder`, `_control`, `_cli`).
  - Test classes inherit `unittest.TestCase` or `unittest.IsolatedAsyncioTestCase`; method names `test_<behavior>_<scenario>`.
  - Optional `if __name__ == "__main__": unittest.main()` at the bottom.

**Example (pattern):**

```python
from __future__ import annotations
import importlib
import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))
_bridge = importlib.import_module("neurohid_ml.bridge")

class BridgeSessionTests(unittest.IsolatedAsyncioTestCase):
    async def test_unsupported_version_emits_protocol_error(self) -> None:
        client = _FakeBridgeClient()
        session = _bridge.BridgeSession(client)
        # ...
        self.assertEqual(client.sent[-1]["kind"], "error")
```

---

## Mocking

### Rust

- **Approach:** No mocking framework. Tests use real types, in-memory or temp resources (e.g. `std::env::temp_dir()`, `std::path::PathBuf::from("/tmp/neurohid_test")` in `crates/neurohid-storage/src/paths.rs`), and small synthetic data (e.g. `sample_snapshot()`). For I/O or external deps, tests either use concrete fakes (e.g. mock device types) or are integration-style where applicable.
- **Fake types:** Some crates expose test-only types (e.g. `crates/neurohid-device/src/mock.rs` has `#[cfg(test)]` usage).

### Python

- **Framework:** `unittest.mock`: `patch`, `patch.object`, `Mock`, used with `autospec=True` where appropriate.
- **Patterns:**
  - Patch a method on the class under test: `with patch.object(_control.NeuroHidControlClient, "_request_endpoint", autospec=True, return_value=...):`
  - Patch a module function: `with patch.object(_trainer.torch.onnx, "export", side_effect=fake_export):`
  - Replace subprocess: `with patch.object(_control.subprocess, "run", autospec=True, return_value=completed):`
- **Fake objects:** In-test classes that implement the minimal interface (e.g. `_FakeBridgeClient` with `send_envelope`, `config`, `connected`, `sent` in `python/tests/test_bridge.py`; `_FakeControl` in `python/tests/test_notebook_helpers.py`). Assign fakes to the object under test when testing integration (e.g. `notebook._control = fake_control`).
- **What to mock:** External I/O (subprocess, IPC, network), and internal methods that would trigger that I/O. Use `autospec=True` to keep signatures realistic.
- **What not to mock:** Pure logic under test; prefer real small data and fakes that mirror the real interface.

---

## Fixtures and Test Data

### Rust

- **Test data:** Built inside `mod tests` (e.g. `sample_snapshot()` in `crates/neurohid-types/src/ipc.rs`, inline struct literals in `crates/neurohid-storage/src/paths.rs` with `/tmp/neurohid_test` or `std::env::temp_dir()`). No shared fixture crate.
- **Float tests:** Use `approx::assert_relative_eq!(value, expected, epsilon = 0.01)` (e.g. `crates/neurohid-signal/src/filter.rs`).

### Python

- **Test data:** Helper functions in the test file (e.g. `_session_payload(sample_count, feature_dim)` in `python/tests/test_trainer.py`). No `conftest.py` or shared fixtures directory detected.
- **Temp resources:** `tempfile.TemporaryDirectory()` and `Path(tmp_dir)` for isolated dirs; `Path(...).write_text(...)` for JSON session logs.
- **Randomness:** Fixed seeds where reproducibility matters (e.g. `np.random.default_rng(7)` in `python/tests/test_decoder_and_errp.py`).

---

## Coverage

### Rust

- **Requirements:** No mandated coverage target. Run tests with `cargo test --workspace`.
- **Coverage tooling:** Not configured in the repo (no `cargo-tarpaulin` or similar in manifests).

### Python

- **Config:** `[tool.coverage.run]`: `branch = true`, `source = ["neurohid_ml"]`; `[tool.coverage.report]`: `show_missing = true`, `skip_covered = true`. Defined in `python/pyproject.toml`.
- **Runner:** pytest-cov (dev dependency `pytest-cov>=7.0.0`). Run with coverage, for example:
  - `uv run --project python pytest python/tests --cov=neurohid_ml --cov-report=term-missing`
- **Target:** No explicit coverage threshold in config; gate sequence in `python/AGENTS.md` does not require a minimum percentage.

---

## Test Types

### Rust

- **Unit tests:** Dominant. Test one module or type in isolation; use `assert!`/`assert_eq!`/`assert_relative_eq!` and small helpers. Many tests verify roundtrips (e.g. IPC envelope encode/decode), path construction, filter math, or state transitions.
- **Integration tests:** No dedicated `tests/` directory at crate level in the sampled layout; multi-crate behavior is covered by unit tests that use types from other crates (e.g. `neurohid_types` in service tests).

### Python

- **Unit tests:** Test one module or class with mocks for I/O and subprocess. Examples: `test_decoder_and_errp.py` (decoder/ErrP logic), `test_bridge.py` (protocol handling with `_FakeBridgeClient`), `test_control_client.py` (control client with patched `_request_endpoint` and subprocess).
- **Async tests:** `unittest.IsolatedAsyncioTestCase` and `async def test_...` for bridge session and message handling.
- **CLI/contract tests:** `test_cli_and_clients.py` tests `_parse_args` and `send_command` with mocked endpoints; `test_notebook_helpers.py` and `test_trainer.py` cover notebook helpers and trainer artifact writing with temp dirs and patched export.

---

## Common Patterns

### Rust

- **Async testing:** Not used in the sampled tests; service and tasks are async but unit tests focus on sync logic or small helpers.
- **Error testing:** `assert!(result.is_err())` or `assert!(result.is_ok())`; sometimes `.expect("message")` in tests where panic is acceptable.
- **Roundtrip tests:** Construct value → serialize/envelope → decode → assert equality (e.g. `crates/neurohid-types/src/ipc.rs`).

### Python

- **Error testing:** `with self.assertRaisesRegex(ExpectedError, "message"): ...`
- **Async testing:** Inherit `unittest.IsolatedAsyncioTestCase` and define `async def test_...(self) -> None:`; no extra event-loop config in the sampled files (pytest-asyncio is available if needed).
- **Import pattern:** Use `sys.path.insert` plus `importlib.import_module` so tests run without `pip install -e .`; one test file (`test_lab_kernel.py`) also does `from neurohid_ml.lab_kernel import ...` after the path insert.

---

## Key File Locations

| Purpose              | Rust                                          | Python                          |
|----------------------|-----------------------------------------------|----------------------------------|
| Test entry / config  | `cargo test`; workspace in root `Cargo.toml`  | `python/pyproject.toml` (pytest, coverage) |
| Test files           | In-tree `#[cfg(test)] mod tests` in `*.rs`    | `python/tests/test_*.py`         |
| Quality gate         | `crates/AGENTS.md` (cargo test, clippy, fmt)  | `python/AGENTS.md` (pytest, ruff, black, mypy) |

---

*Testing analysis: 2026-02-20*
