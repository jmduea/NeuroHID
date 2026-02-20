# Codebase Structure

**Analysis Date:** 2026-02-20

## Directory Layout

```
neurohid/
├── .cursor/              # Editor/agent settings
├── .github/              # CI workflows, hooks, scripts, skills
├── .planning/            # Planning and codebase analysis outputs
│   └── codebase/         # GSD codebase docs (ARCHITECTURE.md, STRUCTURE.md, etc.)
├── crates/               # Rust workspace
│   ├── neurohid/         # Published binary crate (hub + service + validate)
│   │   └── src/
│   │       ├── bin/      # Entry points: neurohid.rs, neurohid-service.rs, neurohid-validate.rs
│   │       └── tracing_init.rs
│   ├── neurohid-calibration/   # Calibration games and wizard
│   │   └── src/          # lib.rs, panel, wizard, games/
│   ├── neurohid-core/    # Task orchestration and runtime
│   │   └── src/          # lib.rs, runtime.rs, service.rs, tasks/
│   ├── neurohid-device/  # EEG device backends (LSL, mock, serial, brainflow)
│   │   └── src/          # lib.rs, traits, lsl/, mock, serial, brainflow
│   ├── neurohid-hub/     # Hub GUI (screens, widgets, workbench)
│   │   └── src/          # lib.rs, app, data_bus, layout, screens/, service_manager, state, stream_console, theme, widgets/, workbench
│   ├── neurohid-ipc/     # IPC v3 transport and broker
│   │   └── src/          # lib.rs, broker, client, protocol, server; bin/ for protocol_encoding_gate
│   ├── neurohid-platform/      # Cross-platform HID emission
│   │   └── src/          # lib.rs, traits, linux, windows, macos
│   ├── neurohid-sdk/     # Feature-gated facade for external Rust users
│   │   └── src/          # lib.rs (re-exports only)
│   ├── neurohid-signal/  # Signal filtering and feature extraction
│   │   └── src/          # lib.rs, buffer, filter, features, pipeline
│   ├── neurohid-storage/ # Encrypted profile/config persistence
│   │   └── src/          # lib.rs, config, credentials, paths, profile, secure
│   └── neurohid-types/   # Shared domain types (no internal deps)
│       └── src/          # lib.rs, action, config, control, device, error, event, ipc, learning, model, observability, observation, profile, reward, signal
├── docs/                 # Architecture, contracts, guides
├── python/               # Python ML package
│   ├── src/
│   │   └── neurohid_ml/  # Package root
│   │       ├── bridge/   # IPC bridge client behavior
│   │       ├── decoder/  # Decoder/model inference
│   │       ├── errp/     # Error-related potential
│   │       ├── trainer/  # Trainer loops and candidate staging
│   │       ├── cli.py    # CLI entrypoint
│   │       ├── control.py, ipc.py, ipc_constants.py, lab_kernel.py, notebook.py, telemetry.py
│   │       └── __init__.py
│   └── tests/            # Pytest tests
├── third_party/          # Third-party assets or vendored deps
├── Cargo.toml            # Workspace root
├── README.md
└── AGENTS.md             # Root agent onboarding
```

## Directory Purposes

**crates/:** Rust workspace. Each subdirectory is one crate. Dependency direction: types → component crates → neurohid-core → (neurohid | neurohid-hub | neurohid-sdk). See `docs/crate-boundaries.md`.

**crates/neurohid/src/bin/:** Binary entry points only. `neurohid.rs` and `neurohid-service.rs` share no code except optional `tracing_init`; service binary is large (CLI, daemon, Windows service). `neurohid-validate.rs` is a separate harness.

**crates/neurohid-core/src/tasks/:** One module per runtime task: `device.rs`, `signal.rs`, `decoder.rs`, `action.rs`, `outlet.rs`, `ipc.rs`, `session_logger.rs`, `latency_alert.rs`, `latency.rs`. Task types and channel structs are in `tasks/mod.rs`.

**crates/neurohid-hub/src/screens/:** One file per screen: `dashboard.rs`, `devices.rs`, `profiles.rs`, `calibration.rs`, `visualization.rs`, `jupyter_ide.rs`, `python_lab.rs`, `settings.rs`. Screen enum and mode-based visibility in `screens/mod.rs`.

**crates/neurohid-hub/src/widgets/:** Reusable UI pieces (e.g. `band_power.rs`, `fft_plot.rs`, `spectrogram.rs`, `time_series.rs`, `decoder_monitor.rs`, `headplot.rs`, `signal_quality.rs`, `stream_metadata.rs`, `channel_meta.rs`, `accelerometer.rs`, `action_preview.rs`, `focus.rs`). Declared in `widgets/mod.rs`.

**python/src/neurohid_ml/:** Top-level package. Subpackages: `bridge/`, `decoder/`, `errp/`, `trainer/`. Top-level modules: `cli.py`, `control.py`, `ipc.py`, `ipc_constants.py`, `lab_kernel.py`, `notebook.py`, `telemetry.py`.

**docs/:** Canonical docs: `index.md` (map), `crate-boundaries.md`, `architecture-rust-core.md`, `architecture-python-ml.md`, `integration-architecture.md`, `protocol-and-api.md`, `development-guide.md`, `deployment-guide.md`.

## Key File Locations

**Entry points:**
- `crates/neurohid/src/bin/neurohid.rs`: Hub GUI
- `crates/neurohid/src/bin/neurohid-service.rs`: Headless service (and Windows service entry)
- `crates/neurohid/src/bin/neurohid-validate.rs`: Validation harness
- `python/src/neurohid_ml/cli.py`: Python CLI (`main`)

**Configuration:**
- `Cargo.toml`: Workspace members and shared deps
- `python/pyproject.toml`: Python project and script entry
- Rust config types: `crates/neurohid-types/src/config.rs`
- Storage paths/config: `crates/neurohid-storage/src/config.rs`, `paths.rs`

**Core logic:**
- Runtime build/handle: `crates/neurohid-core/src/runtime.rs`
- Service and task spawning: `crates/neurohid-core/src/service.rs`
- Task implementations: `crates/neurohid-core/src/tasks/*.rs`
- Hub app and screen dispatch: `crates/neurohid-hub/src/app.rs`
- IPC protocol and transport: `crates/neurohid-ipc/src/protocol.rs`, `broker.rs`, `server.rs`, `client.rs`
- Device traits and LSL: `crates/neurohid-device/src/traits.rs`, `crates/neurohid-device/src/lsl/`
- Signal pipeline: `crates/neurohid-signal/src/pipeline.rs`, `filter.rs`, `features.rs`
- Python bridge: `python/src/neurohid_ml/bridge/__init__.py`; IPC client: `python/src/neurohid_ml/ipc.py`; control client: `python/src/neurohid_ml/control.py`

**Testing:**
- Rust: tests in crate `src/` or `tests/` per crate; SDK has inline `#[cfg(test)]` in `crates/neurohid-sdk/src/lib.rs`
- Python: `python/tests/` (e.g. `test_bridge.py`, `test_control_client.py`, `test_decoder_and_errp.py`, `test_trainer.py`, `test_cli_and_clients.py`, `test_notebook_helpers.py`, `test_lab_kernel.py`)
- Validation binary: `crates/neurohid/src/bin/neurohid-validate.rs` (soak/latency/boot matrix)

## Naming Conventions

**Rust:**
- Crates: `neurohid-<layer>` (kebab-case)
- Library names: `neurohid_<layer>` (snake_case in Cargo.toml `[lib] name =`)
- Binaries: `neurohid`, `neurohid-service`, `neurohid-validate`
- Files: `snake_case.rs`; modules mirror type or domain (e.g. `runtime.rs`, `service.rs`, `tasks/device.rs`)
- Types/functions: `PascalCase` for types/traits, `snake_case` for functions/variables (per `crates/AGENTS.md`)

**Python:**
- Package: `neurohid_ml` (snake_case)
- Modules: `snake_case.py` (`cli.py`, `control.py`, `ipc.py`, etc.)
- Subpackages: `bridge`, `decoder`, `errp`, `trainer`
- Script entry: `neurohid-ml` (CLI name in pyproject.toml)

**Docs:**
- `docs/*.md` for architecture, protocol, guides; `docs/crate-boundaries.md` and `docs/index.md` are canonical references.

## Where to Add New Code

**New Rust feature (domain logic):**
- Shared types/schemas: `crates/neurohid-types/src/` (new module or extend existing)
- Device backend: `crates/neurohid-device/src/` (new file + trait impl; optional feature in Cargo.toml)
- Signal step: `crates/neurohid-signal/src/` (filter/features/pipeline)
- New runtime task: `crates/neurohid-core/src/tasks/<name>.rs` and register in `service.rs` and `tasks/mod.rs`
- New hub screen: `crates/neurohid-hub/src/screens/<name>.rs`, add variant to `Screen` in `screens/mod.rs`, wire in `app.rs`
- New hub widget: `crates/neurohid-hub/src/widgets/<name>.rs` and `widgets/mod.rs`

**New binary:**
- Add `[[bin]]` in `crates/neurohid/Cargo.toml` and add `src/bin/<name>.rs`

**New Python subpackage or CLI command:**
- Subpackage: `python/src/neurohid_ml/<name>/` with `__init__.py`
- CLI: extend `cli.py` subparsers and dispatch; optional new module for command logic
- Tests: `python/tests/test_<area>.py`

**New crate:**
- Add member under `crates/<name>/` with `Cargo.toml` and `src/lib.rs` (or bins); add to workspace `Cargo.toml` `members`; follow `docs/crate-boundaries.md` for layer and dependency direction.

## Special Directories

**.github/:** CI workflows (`.github/workflows/`), hooks, scripts, and skill definitions. Committed.

**.planning/codebase/:** GSD-generated analysis (e.g. ARCHITECTURE.md, STRUCTURE.md, STACK.md). Committed.

**target/:** Cargo build output. Not committed. Ignore in exploration.

**python/.venv/, __pycache__, .mypy_cache/, .pytest_cache/, .ruff_cache/:** Python tooling artifacts. Not committed (or optionally .venv for local use).

**third_party/:** Vendored or third-party assets. Committed as needed.

---

*Structure analysis: 2026-02-20*
