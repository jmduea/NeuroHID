# Technology Stack

**Analysis Date:** 2026-02-20

## Languages

**Primary:**
- Rust (edition 2024, rust-version 1.85) — runtime, device/signal/action stack, IPC, GUI, SDK in `crates/`
- Python (>=3.12) — ML bridge, decoder/ErrP/trainer flows, notebooks in `python/`

**Secondary:**
- PowerShell — repository automation under `.github/scripts/` (e.g. `run-agent-ready-tasks.ps1`, `classify-impact.ps1`)

## Runtime

**Environment:**
- Rust: stable toolchain (CI uses `dtolnay/rust-toolchain@stable`); development requires Rust 1.85+ per `docs/development-guide.md`
- Python: 3.12+; classifiers include 3.12, 3.13, 3.14 in `python/pyproject.toml`; CI uses `actions/setup-python@v5` with `python-version: "3.12"`

**Package Manager:**
- Cargo — workspace root `Cargo.toml`, lockfile `Cargo.lock` present
- uv — Python; `uv sync --directory python`, `uv run --project python`; lockfile `python/uv.lock` present

## Frameworks

**Core:**
- egui 0.33.3 / eframe 0.33.3 — GUI for Hub and calibration; platform features: `default_fonts`, `glow`, `x11` (Unix); `crates/neurohid`, `crates/neurohid-hub`, `crates/neurohid-calibration`
- Tokio 1.49 (full) — async runtime across Rust crates
- tract-onnx 0.22 — ONNX inference in Rust decoder (`crates/neurohid-core/src/tasks/decoder.rs`)

**Testing:**
- Rust: `approx` 0.5.1 (workspace), `tokio-test` 0.4 in crates
- Python: pytest >=9.0.2, pytest-cov >=7.0.0, pytest-asyncio >=1.3.0

**Build/Dev:**
- setuptools >=61.0, wheel — Python build backend in `python/pyproject.toml`
- black >=26.1.0, ruff >=0.15.0, mypy >=1.19.1 — Python lint/format/type-check (optional dev deps)
- clap 4.4 (derive) — CLI for `neurohid` binaries in `crates/neurohid`
- CI: GitHub Actions (checkout@v4, astral-sh/setup-uv@v4, Swatinem/rust-cache@v2, codecov/codecov-action@v5)

## Key Dependencies

**Critical (Rust workspace):**
- `serde` 1.0.228 (derive), `serde_json` 1.0.149, `toml` 0.9.8 — serialization
- `ipckit` 0.1.6 (async, backend-interprocess) — IPC transport (named pipe / local socket); `crates/neurohid-ipc`, Python `ipckit>=0.1.6`
- `thiserror` 2.0.18, `anyhow` 1.0.101 — error handling
- `tracing` 0.1.44, `tracing-subscriber` 0.3.22 (env-filter, json) — logging
- `ndarray` 0.17.2, `num-complex` 0.4.6, `rustfft` 6.4.1 — numeric/signal processing
- `keyring` 3.6.3 (linux-native, apple-native, windows-native), `aes-gcm` 0.10.3, `rand` 0.10.0, `base64` 0.22.1 — secure storage in `crates/neurohid-storage`
- `chrono` 0.4.43 (serde) — time
- `lsl` 0.1 (optional, via `neurohid-device`) — Lab Streaming Layer; `lsl-sys` patched from git in workspace `[patch.crates-io]`
- `serialport` 4.7.3 — serial device backend in `crates/neurohid-device`
- `enigo` 0.6.1 — cross-platform input simulation (HID) in `crates/neurohid-platform`
- `prost` 0.13 — protobuf (IPC) in `crates/neurohid-ipc`
- `async-trait` 0.1 — async traits
- `dirs` 6.0.0 — platform data dirs
- `tokio-tungstenite` 0.28 (native-tls) — workspace dep (present in root `Cargo.toml`, not yet used in code)
- `windows` 0.62.2, `core-graphics` 0.25, `core-foundation` 0.10.1 — platform-specific in `crates/neurohid-platform`
- `windows-service` 0.7 — Windows service in `crates/neurohid` (neurohid-service binary)
- Hub/calibration: `armas` 0.1.2, `egui_dock` 0.18.0, `egui-async` 0.3.4, `egui_code_editor` 0.2.20, `egui_console` 0.3.1, `egui_logger` 0.9.2

**Critical (Python):**
- `torch` >=2.10.0 — ML models in `python/src/neurohid_ml/decoder`, `python/src/neurohid_ml/trainer`
- `onnx` >=1.19.0 — ONNX export/artifact handling
- `numpy` >=2.4.2, `scipy` >=1.17.0, `scikit-learn` >=1.8.0 — numerics/ML in decoder, errp, trainer
- `jupyterlab` >=4.4.0 — notebooks
- `ipckit` >=0.1.6 — IPC client in `python/src/neurohid_ml/ipc.py`, `python/src/neurohid_ml/bridge/__init__.py`

**Optional / runtime-only (Python):**
- `joblib` — used in `python/src/neurohid_ml/errp/__init__.py` for loading ErrP detector from file (import inside method)
- `pandas` — required for `events_to_dataframe()` in `python/src/neurohid_ml/ipc.py` when used (import inside function)

## Configuration

**Environment:**
- No `.env` files committed. Runtime behavior via env: `NEUROHID_LOG_FORMAT` (json/text) in `crates/neurohid/src/tracing_init.rs`; `NEUROHID_SERVICE_BIN`, `NEUROHID_NOTIFY_TITLE`, `NEUROHID_NOTIFY_BODY` for service/notifications in `crates/neurohid-hub/src/app.rs`, `crates/neurohid/src/bin/neurohid-validate.rs`; WSL: `WSL_DISTRO_NAME`, `WSLENV`, `WINIT_UNIX_BACKEND` (x11) in `crates/neurohid/src/bin/neurohid.rs`. Log filter: `RUST_LOG` (e.g. `RUST_LOG=neurohid=debug`). See `docs/deployment-guide.md`.
- Python: `uv` manages environment; no project-level env file required for core flows.

**Build:**
- Rust: workspace `Cargo.toml` + per-crate `Cargo.toml`; workspace lints: `unsafe_code = "warn"`, `unused = "warn"`, clippy `all = "warn"`, `pedantic = "warn"`.
- Python: `python/pyproject.toml` (setuptools); black line-length 100, py312; ruff line-length 100, select E/F/I/N/W; mypy 3.12, warn_return_any, warn_unused_configs; coverage branch=true, source neurohid_ml.

## Platform Requirements

**Development:**
- Rust 1.85+, Python 3.12+, uv, PowerShell for automation. LSL: install `liblsl-dev` (e.g. Linux) and use workspace-pinned `lsl-sys` for reproducible builds. See `docs/development-guide.md`.

**Production:**
- Local/desktop or headless service; Windows service support via `neurohid-service`. IPC: named pipe (Windows) or TCP loopback (e.g. 127.0.0.1:47384). No mandatory cloud or external services. See `docs/deployment-guide.md`.

---

*Stack analysis: 2026-02-20*
