# Coding Conventions

**Analysis Date:** 2026-02-20

NeuroHID is a hybrid Rust/Python monorepo. Conventions are defined per lane; root baseline is in `AGENTS.md`, with lane overrides in `crates/AGENTS.md` and `python/AGENTS.md`.

---

## Naming Patterns

### Rust (`crates/`)

- **Variables, functions, modules:** `snake_case`
- **Types, traits:** `PascalCase`
- **Constants:** `SCREAMING_SNAKE_CASE`
- **Files:** `snake_case.rs`; one main module per file matching the filename (e.g. `paths.rs` → `mod paths`)

Reference: `crates/AGENTS.md` (Coding Standards).

### Python (`python/`)

- **Modules/packages:** `snake_case` (e.g. `neurohid_ml`, `bridge`, `decoder`)
- **Classes:** `PascalCase` (e.g. `BridgeSession`, `DecoderConfig`, `NeuroHidControlClient`)
- **Functions, variables:** `snake_case`
- **Constants:** `UPPER_SNAKE` in shared constants (e.g. `CANONICAL_IPC_MODE` in `python/src/neurohid_ml/ipc_constants.py`)
- **Test files:** `test_<module_or_feature>.py` under `python/tests/`
- **Private test helpers:** leading underscore (e.g. `_FakeBridgeClient`, `_session_payload`, `_FakeControl`)

---

## Code Style

### Rust

- **Formatting:** `cargo fmt` (default rustfmt; no project `rustfmt.toml` detected). Max line length: 100 characters (per `crates/AGENTS.md`).
- **Linting:** Workspace lints in root `Cargo.toml`: `unsafe_code = "warn"`, `unused = "warn"`; Clippy: `all = "warn"`, `pedantic = "warn"`. Run: `cargo clippy --workspace -- -D warnings`.
- **Edition:** 2024. **Rust version:** 1.85.

### Python

- **Formatting:** Black, line-length 100, target Python 3.12. Config in `python/pyproject.toml` (`[tool.black]`).
- **Linting:** Ruff with `select = ["E", "F", "I", "N", "W"]`, line-length 100. Config in `python/pyproject.toml` (`[tool.ruff]`).
- **Type checking:** mypy, python_version 3.12, `warn_return_any = true`, `warn_unused_configs = true`; overrides for `scipy`/`sklearn` ignore missing imports. Config in `python/pyproject.toml` (`[tool.mypy]`).

---

## Import Organization

### Rust

- Standard library first, then external crates, then internal crate/crate-relative (`crate::`, `super::`). Example from `crates/neurohid-core/src/service.rs`: `use std::...; use tokio::...; use neurohid_storage::...; use neurohid_types::...; use crate::tasks::...;`
- Test modules: `use super::*;` and `use crate::...` for types under test. Helpers (e.g. `sample_snapshot()`) live inside `#[cfg(test)] mod tests`.

### Python

- `from __future__ import annotations` at the top of files (used in `python/src/neurohid_ml/` and `python/tests/`).
- Then standard library, then third-party, then local. Tests often use `importlib.import_module("neurohid_ml.<module>")` and assign to a private name (e.g. `_bridge = importlib.import_module("neurohid_ml.bridge")`) to avoid installing the package when running tests with `sys.path.insert(0, src)`.
- No path aliases in use; imports are package-relative (e.g. `from neurohid_ml.errp import ErrPConfig, ErrPDetector`).

---

## Error Handling

### Rust

- **Strategy:** Recoverable errors use `Result<T, E>` and `?`; avoid `unwrap()` in library paths. Panics are avoided in recoverable library code.
- **Error types:** Centralized in `crates/neurohid-types/src/error.rs`. Use `thiserror` for enum errors with `#[error("...")]` messages. Subsystems have their own enums (`DeviceError`, `SignalError`, `StorageError`, `IpcError`, etc.); top-level `Error` wraps them and implements `From` for `?` propagation. Convenience alias: `neurohid_types::error::Result<T>`.
- **Context at boundaries:** Use `.map_err(|e| SomeError::Variant(e.to_string()))` (or similar) when converting at API boundaries (e.g. in `crates/neurohid-platform/src/linux.rs`, `crates/neurohid-device/src/lsl/provider.rs`).
- **Binaries:** `anyhow` used for application-level error handling (e.g. `crates/neurohid/src/tracing_init.rs`).
- **Unsafe:** Every `unsafe` block must include a `// SAFETY:` comment. Keep unsafe localized and behind safe APIs. Policy in root `AGENTS.md` and `crates/AGENTS.md`.

### Python

- **Strategy:** Exceptions for error paths; custom exception types for API boundaries (e.g. `NotebookError` in `neurohid_ml.control`). Tests use `self.assertRaisesRegex(ExpectedError, "message")`.
- **Validation:** Runtime checks with clear errors (e.g. `IpcConfig.__post_init__` in `python/src/neurohid_ml/bridge/__init__.py` raises `RuntimeError` for unsupported `ipc_mode`).

---

## Logging

### Rust

- **Framework:** `tracing` and `tracing-subscriber`. Used across crates (e.g. `crates/neurohid-core/src/service.rs`, `crates/neurohid-core/src/tasks/device.rs`, `crates/neurohid-ipc/src/server.rs`, `crates/neurohid/src/bin/neurohid-service.rs`). Initialization in `crates/neurohid/src/tracing_init.rs` (`init_tracing(default_level)`).

### Python

- **Approach:** Standard library `logging` or ad-hoc logging where needed; no single logging convention enforced in the sampled modules. Use appropriate levels and avoid logging secrets.

---

## Comments

### Rust

- **Module-level:** `//!` doc comments describing the module’s role (e.g. `crates/neurohid-types/src/ipc.rs`, `crates/neurohid-core/src/service.rs`).
- **Items:** `///` for public types and functions. No mandate for every function; used for public API and non-obvious behavior.
- **SAFETY:** Required for every `unsafe` block.

### Python

- **Modules:** Module docstrings (e.g. `python/src/neurohid_ml/bridge/__init__.py`, `python/src/neurohid_ml/decoder/__init__.py`) describing purpose and behavior.
- **Classes/dataclasses:** Docstrings for config and important types (e.g. `DecoderConfig`, `IpcConfig`).
- **Pragmas:** `# noqa: SLF001` used in tests when intentionally accessing private helpers (e.g. `python/tests/test_control_client.py`). `# pragma: no cover` for optional dependency branches.

---

## Function Design

### Rust

- Prefer small, focused functions. Use `?` for propagation; add context with `.map_err()` at boundaries. Return `Result` from fallible public APIs.

### Python

- Type hints on public functions and test methods (e.g. `def test_unsupported_version_emits_protocol_error(self) -> None:`). Use `Sequence[str] | None`-style unions and `list`, `dict` from builtins where supported (Python 3.12+).

---

## Module Design

### Rust

- **Exports:** Public API via `pub`; re-exports used for convenience (e.g. `pub type BandpassFilter = FilterType` in `crates/neurohid-signal/src/filter.rs`). Crate boundaries documented in `docs/crate-boundaries.md`.
- **Tests:** Co-located in the same file under `#[cfg(test)] mod tests { ... }`; no separate `tests/` directory for unit tests.

### Python

- **Packages:** `src/neurohid_ml/` with subpackages `bridge`, `decoder`, `errp`, `trainer`, etc. Entrypoint: `neurohid_ml.cli:main` (script `neurohid-ml` in `python/pyproject.toml`).
- **Exports:** Public types and functions imported by other modules or tests; internal helpers not re-exported. Tests use `importlib` + `sys.path` to load `neurohid_ml` without installing.

---

*Convention analysis: 2026-02-20*
