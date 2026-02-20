# Architecture

**Analysis Date:** 2026-02-20

## Pattern Overview

**Overall:** Layered monorepo with Rust runtime + Python ML bridge. Local-process integration via IPC; dependency flow is strictly downward from UI/entrypoints through orchestration to component crates and shared types.

**Key Characteristics:**
- Rust workspace: 11 crates in four layers (types → components → core → UI/binary/SDK)
- Python package: single `neurohid_ml` with bridge, decoder, errp, trainer, CLI
- Communication: local transport (named pipe on Windows, TCP loopback elsewhere); Rust emits events, Python returns ML/ErrP/training results
- Task-based runtime: `NeuroHidService` spawns concurrent tasks (device, signal, decoder, action, outlet, ipc, session_logger, latency_alert) communicating via channels

## Layers

**Shared types (bottom):**
- Purpose: Domain/config/control/signal/action/IPC types only; no runtime or UI
- Location: `crates/neurohid-types/`
- Contains: `action`, `config`, `control`, `device`, `error`, `event`, `ipc`, `learning`, `model`, `observability`, `observation`, `profile`, `reward`, `signal`
- Depends on: minimal (chrono, thiserror, serde)
- Used by: all other Rust crates

**Runtime component crates:**
- Purpose: Isolated capabilities (device backends, signal pipeline, platform HID, IPC transport, storage, calibration)
- Location: `crates/neurohid-device/`, `crates/neurohid-signal/`, `crates/neurohid-platform/`, `crates/neurohid-ipc/`, `crates/neurohid-storage/`, `crates/neurohid-calibration/`
- Contains: trait-based device/provider abstractions, filter/feature pipeline, platform-specific HID, IPC broker/client/server, encrypted persistence, calibration games/wizard
- Depends on: `neurohid-types` (and optional backends, e.g. LSL)
- Used by: `neurohid-core` (and `neurohid-hub`/`neurohid` only via core facade where applicable)

**Composition/orchestration:**
- Purpose: Wire components into end-to-end runtime; task supervisor pattern
- Location: `crates/neurohid-core/`
- Contains: `runtime.rs` (RuntimeBuilder, RuntimeHandle, RuntimeCommand), `service.rs` (NeuroHidService, DeviceCommand, SignalCommand, DecoderCommand, IntegrityStage), `tasks/` (device, signal, decoder, action, outlet, ipc, session_logger, latency_alert, latency)
- Depends on: neurohid-types, neurohid-device, neurohid-signal, neurohid-platform, neurohid-storage, neurohid-ipc
- Used by: `neurohid` (binaries), `neurohid-hub`, `neurohid-sdk` (feature-gated)

**UI and entrypoints:**
- Purpose: Desktop GUI (Hub), headless service, validation harness, published SDK facade
- Location: `crates/neurohid/` (bins), `crates/neurohid-hub/`, `crates/neurohid-sdk/`
- Contains: HubApp, screens (dashboard, devices, profiles, calibration, visualization, python_lab, jupyter_ide, settings), widgets, workbench, service_manager; SDK re-exports
- Depends on: neurohid-core, neurohid-hub (for main binary), neurohid-ipc, neurohid-storage, neurohid-types; hub uses neurohid-core facade for IPC/storage
- Used by: end users and embedders

**Python ML package:**
- Purpose: IPC bridge client, decoder/ErrP logic, trainer workflows, CLI and notebook integration
- Location: `python/src/neurohid_ml/`
- Contains: `bridge/`, `decoder/`, `errp/`, `trainer/`, `cli.py`, `control.py`, `ipc.py`, `ipc_constants.py`, `lab_kernel.py`, `notebook.py`, `telemetry.py`
- Depends on: torch, onnx, numpy, scipy, scikit-learn, jupyterlab, ipckit
- Used by: CLI (`neurohid-ml`), notebooks, and Rust runtime (via IPC)

## Data Flow

**Signal → HID:**
1. Device backends (LSL/Mock/Serial/BrainFlow) produce raw samples via `Device`/`DeviceProvider` traits in `crates/neurohid-device/`
2. `DeviceTask` in `crates/neurohid-core/src/tasks/device.rs` forwards samples to `SignalTask`
3. `SignalTask` (`crates/neurohid-core/src/tasks/signal.rs`) uses `neurohid-signal` pipeline (filter, features) and sends `FeatureVector` to `DecoderTask`
4. `DecoderTask` (`crates/neurohid-core/src/tasks/decoder.rs`) runs inference (tract-onnx), emits decisions and `DecisionEventRecord` to IPC and `ActionTask`
5. `ActionTask` (`crates/neurohid-core/src/tasks/action.rs`) produces `Action`; `OutletTask` (`crates/neurohid-core/src/tasks/outlet.rs`) emits HID via `neurohid-platform`

**Runtime ↔ Python bridge:**
1. Runtime sends `DecisionEvent` + `RuntimeTelemetry` to Python bridge over IPC
2. Python bridge (`python/src/neurohid_ml/bridge/`) consumes events, runs decoder/ErrP, returns `ErrPResult`, `TrainerStatus`, `CandidateModelReady`
3. Runtime applies outputs and updates control/telemetry state; reconnect/fallback when bridge unavailable

**Control:**
1. Clients (Hub or CLI) send `ControlRequest` (e.g. snapshot, set_output_enabled) to runtime
2. Runtime responds with `ControlSnapshot` / `TrainerSnapshot`; control uses same IPC transport (control.rpc channel)

**State management:** Configuration and profile state live in `neurohid-storage`; runtime holds `SystemConfig`, `ProfileId`, and task-specific channels. Hub holds `HubState`, `DataBus`, `ServiceManager`, and screen state; it talks to embedded or external runtime via `neurohid_core::facade` (IPC/storage) and `RuntimeHandle`/`RuntimeCommand`.

## Key Abstractions

**Device layer:**
- Purpose: Uniform discovery, connect, and stream from biosensor backends
- Traits: `DeviceProvider`, `Device`, `SampleStream` in `crates/neurohid-device/src/traits.rs`
- Implementations: `crates/neurohid-device/src/lsl/`, `mock.rs`, `serial.rs`, `brainflow.rs`
- Pattern: Provider discovers/connects; Device represents connected stream and yields `Sample`

**IPC envelope:**
- Purpose: Versioned, channel-carrying message envelope for all Rust↔Python and control traffic
- Types: `IpcEnvelope`, `IpcChannel`, `RuntimeEvent`, `ControlRpcRequest`/`ControlRpcResponse` in `crates/neurohid-types/src/ipc.rs`; transport in `crates/neurohid-ipc/`
- Pattern: JSON over local transport; protocol v3; channels: `control.rpc`, `trainer.stream`, `runtime.events`

**Managed runtime:**
- Purpose: Single entry to start/stop and command the service from hub or headless binary
- Types: `RuntimeBuilder`, `RuntimeHandle`, `RuntimeCommand`, `RuntimeSnapshot` (= `ControlSnapshot`) in `crates/neurohid-core/src/runtime.rs`
- Pattern: Builder config + optional ProfileStore/ProfileId → `start()` returns handle; commands sent via handle

**Service/task graph:**
- Purpose: Conductor of concurrent tasks; no reverse dependency from components to core
- Type: `NeuroHidService` in `crates/neurohid-core/src/service.rs`; tasks in `crates/neurohid-core/src/tasks/mod.rs`
- Pattern: Spawns DeviceTask, SignalTask, DecoderTask, ActionTask, OutletTask, IpcTask, SessionLoggerTask, LatencyAlertMonitorTask; channels between them; integrity stages (Device, Signal, Decoder, Action, Ipc) rolled up for observability

**Hub app and screens:**
- Purpose: Single eframe app with sidebar, workbench, and screen dispatch
- Type: `HubApp` in `crates/neurohid-hub/src/app.rs`; `Screen` enum and per-screen modules in `crates/neurohid-hub/src/screens/`
- Pattern: `HubApp` owns tokio runtime, `HubState`, `ServiceManager`, `DataBus`, `StreamConsole`, and one instance per screen; sidebar selects `Screen`; workbench manages activity lanes and bottom tabs

## Entry Points

**Rust binaries:**
- `crates/neurohid/src/bin/neurohid.rs`: GUI entry; initializes logging, tokio runtime, eframe; runs `HubApp::new(cc, runtime)` via `eframe::run_native`
- `crates/neurohid/src/bin/neurohid-service.rs`: Headless service; CLI (clap) for run vs Windows service vs daemon commands; builds `RuntimeBuilder`/`NeuroHidService`, starts IPC broker and control server; on Windows can install/start/stop service
- `crates/neurohid/src/bin/neurohid-validate.rs`: Validation harness; subcommands (Soak, LatencyMatrix, BootMatrix); spawns `neurohid-service`, sends control requests, collects snapshots

**Rust library entry (embedding):**
- `crates/neurohid-core/src/lib.rs`: Exposes `runtime`, `service`, `tasks`, and `facade` (re-exports of IPC and storage types for hub/embedders)

**Python:**
- `python/src/neurohid_ml/cli.py`: Entrypoint `main()`; subparsers for `bridge`, `control`, `train-profile-candidate`, `trainer-worker`, etc.; `[project.scripts] neurohid-ml = "neurohid_ml.cli:main"` in `python/pyproject.toml`

## Error Handling

**Strategy:** Result-based; `neurohid_types::error::Error` and `Result<T>` used across crates. Recoverable paths use `?`; panics avoided in library code. Unsafe blocks require `// SAFETY:` comments.

**Patterns:**
- Service task failure triggers graceful shutdown (supervisor pattern)
- IPC/bridge: runtime continues when Python bridge unavailable; warn+degrade integrity
- Hub init: on storage failure, fallback state is created and error surfaced without aborting

## Cross-Cutting Concerns

**Logging:** `tracing` + `tracing-subscriber`; `NEUROHID_LOG_FORMAT` for JSON vs human; hub also uses `egui_logger` and `tracing_log::LogTracer` in `crates/neurohid/src/bin/neurohid.rs`

**Validation:** Config and contract types in `neurohid-types`; IPC schema and defaults in `neurohid-ipc`/`neurohid-types`; `docs/protocol-and-api.md` is protocol reference

**Authentication:** Local-only; no cloud auth. Storage uses OS keyring (keyring crate) for key material; profile/config on disk (encrypted where specified)

---

*Architecture analysis: 2026-02-20*
