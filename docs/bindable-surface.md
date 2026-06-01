# Intended Python-Bindable Surface

**Status:** Implemented (v1.3 — `neurohid-py` crate with PyO3/maturin, see [ADR-001](adr/ADR-001-in-process-python-bindings.md))  
**Purpose:** Documents the embedder-facing Rust API items exposed in the Python bindings.

> This document originated as BIND-02 output. The binding layer is now implemented in `crates/neurohid-py/` with module name `neurohid`.

---

## Scope

The Python bindings milestone (v1.3+) aims to let Python scripts and Jupyter notebooks run the NeuroHID managed runtime, subscribe to signal/action streams, and trigger decoder/recording commands — without writing any Rust.

The bindable surface is intentionally narrow: it covers the **managed runtime path** only. Lower-level crates (`neurohid-device`, `neurohid-signal`) are accessible via the runtime and do not need direct Python bindings.

---

## Bindable Crates

| Crate | Binding priority | Notes |
|---|---|---|
| `neurohid-types` | High | Shared domain types used in every API call |
| `neurohid-core` (runtime module) | High | `RuntimeBuilder`, `RuntimeHandle`, `RuntimeCommand`, `RuntimeSnapshot` |
| `neurohid-ipc` (protocol/client) | Medium | `IpcClient`, `IpcConfig`, `IpcTransport` for external-mode control |
| `neurohid-storage` (paths/config/profile) | Medium | `DataPaths`, `ConfigStore`, `ProfileStore` for setup |

---

## Intended Bindable Items

### `neurohid_types` — Core domain types

| Item | Kind | Purpose |
|---|---|---|
| `SystemConfig` | struct | Top-level runtime configuration |
| `ProfileId` | newtype | Identifies a decoder profile |
| `DeviceId` | newtype | Identifies a biosensor device |
| `ConnectionState` | enum | Device connection lifecycle |
| `Action` | enum | Decoded HID output action |
| `Key` | enum | Keyboard key variant |
| `MouseButton` | enum | Mouse button variant |
| `signal::Sample` | struct | Raw EEG sample (channel values + timestamp) |
| `signal::FeatureVector` | struct | Extracted signal features |
| `signal::ChannelId` | newtype | EEG channel identifier |
| `control::ControlRequest` | struct | IPC control envelope |
| `control::ControlCommand` | enum | IPC control commands (Snapshot, SetOutputEnabled, …) |
| `control::ControlSnapshot` | struct | Runtime snapshot response |

### `neurohid_core::runtime` — Managed runtime

| Item | Kind | Purpose |
|---|---|---|
| `RuntimeBuilder` | struct | Constructs a managed runtime from `SystemConfig` |
| `RuntimeBuilder::new(config)` | fn | Entry point for embedders |
| `RuntimeBuilder::with_profile_store(store)` | fn | Attach profile store |
| `RuntimeBuilder::with_profile_id(id)` | fn | Select active profile |
| `RuntimeBuilder::spawn()` | async fn | Start the runtime, returns `RuntimeHandle` |
| `RuntimeHandle` | struct | Live handle to a running runtime |
| `RuntimeHandle::command(cmd)` | async fn | Send a `RuntimeCommand` |
| `RuntimeHandle::snapshot()` | async fn | Read current `RuntimeSnapshot` |
| `RuntimeHandle::is_running()` | fn | Check if runtime is still alive |
| `RuntimeHandle::shutdown()` | async fn | Graceful shutdown |
| `RuntimeHandle::subscribe_samples()` | fn | Subscribe to live `Sample` broadcast |
| `RuntimeHandle::subscribe_features()` | fn | Subscribe to live `FeatureVector` broadcast |
| `RuntimeHandle::subscribe_actions()` | fn | Subscribe to live `Action` broadcast |
| `RuntimeCommand` | enum | Commands: `SetOutputEnabled`, `SetCalibrationMode`, `ReloadModel`, … |
| `RuntimeSnapshot` | struct | Point-in-time runtime state |
| `RuntimeIpcHandle` | struct | IPC client handle for external-mode control |

### `neurohid_ipc` — IPC control (external-mode)

| Item | Kind | Purpose |
|---|---|---|
| `IpcClient` | struct | Blocking IPC client for sending control commands |
| `IpcConfig` | struct | IPC endpoint configuration |
| `IpcTransport` | enum | Transport selection (Unix socket / TCP loopback) |
| `send_control_request_blocking(config, req)` | fn | Send a `ControlRequest` and receive `ControlResponse` |

### `neurohid_storage` — Setup helpers

| Item | Kind | Purpose |
|---|---|---|
| `DataPaths` | struct | Locates platform-specific data directories |
| `DataPaths::new(override_root)` | fn | Construct from optional override path |
| `ConfigStore` | struct | Load/save `SystemConfig` |
| `ProfileStore` | struct | Manage decoder profiles |
| `SecureStorage` | struct | Secrets/credentials (e.g., license tokens) |
| `initialize(paths)` | fn | One-shot storage setup |

---

## Items Intentionally Excluded

| Item | Reason |
|---|---|
| `neurohid-platform` (all) | Internal OS/HID emulation layer; not re-exported by facade; accessed only through `RuntimeHandle` |
| `neurohid_core::tasks` (all) | Internal pipeline task structs; not part of the stable embedder API (`#[doc(hidden)]`) |
| `neurohid_core::service::ServiceHandle` fields | Internal wiring; embedders use `RuntimeHandle` instead |
| `neurohid-signal` raw structs | Accessible via `RuntimeHandle::subscribe_*` callbacks; no direct binding needed |
| `neurohid-device` raw traits | Device lifecycle managed by runtime; no direct binding needed for MVP |
| `neurohid-outlet-example` | Example extension; not published |

---

## Design Notes for Binding Implementation

1. **Entry point:** `RuntimeBuilder::new(config).spawn()` is the single async entry point. In Python this becomes `await neurohid.runtime.build(config).spawn()`.

2. **Async:** `RuntimeHandle` methods are async. The Python binding should use `asyncio`-compatible wrappers (`pyo3-asyncio` or `pyo3-tokio`).

3. **Broadcast streams:** `subscribe_samples()` / `subscribe_features()` / `subscribe_actions()` return broadcast receivers. In Python these should become async iterators or callback registrations.

4. **Config serialization:** `SystemConfig` is `serde::Serialize/Deserialize`. Consider exposing a `from_dict` / `to_dict` helper in the binding layer rather than mapping every field.

5. **Error types:** `neurohid_types::error::Error` is the top-level error type. Map it to a Python `NeurohidError` exception hierarchy.

6. **Thread safety:** All bindable items use `Arc` / `tokio` sync primitives and are `Send + Sync`. The binding targets free-threaded Python (3.14+); no GIL interaction is used.

---

*Document created: 2026-02-22 (BIND-02)*  
*Last updated: 2026-02-22 after BIND-01 audit*
