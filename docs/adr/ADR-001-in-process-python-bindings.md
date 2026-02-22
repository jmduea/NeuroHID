# ADR-001: In-Process Python Bindings

- **Status:** Accepted
- **Date:** 2026-02-22
- **Author:** NeuroHID team

## Context

The current `neurohid-py` crate exposes a single `IpcChannel` class that communicates
with the NeuroHID runtime over local sockets (named pipes on Windows, Unix domain
sockets elsewhere) using `ipckit::AsyncLocalSocketStream`. Every Python↔Rust
interaction serializes data as JSON, frames it with a 4-byte length prefix, and
pushes it through a socket to a separate service process.

This architecture has several drawbacks:

1. **Latency:** Every call pays socket round-trip + JSON serialization cost.
2. **No true in-process execution:** Python cannot embed the runtime directly;
   it must connect to a running service.
3. **Limited free-threading benefit:** While the code uses `py.detach()` to
   release the GIL during I/O, the actual parallelism is limited to the
   socket wait — the runtime itself runs in a separate process.
4. **Narrow API surface:** Only raw envelope send/receive is exposed, forcing
   the Python side to maintain its own control/bridge/event protocol logic.

The `docs/bindable-surface.md` specification (BIND-02 output) defines the
intended Python-bindable surface: `RuntimeBuilder`, `RuntimeHandle`,
`RuntimeCommand`, domain types, and broadcast stream subscriptions — all via
direct in-process calls.

## Decision

Replace the socket-based IPC shim with in-process PyO3 bindings that embed the
NeuroHID runtime directly in the Python process:

1. **`neurohid-py` depends on `neurohid-core`, `neurohid-types`, and
   `neurohid-storage`** instead of `ipckit`. The crate becomes a runtime
   embedder, not a transport shim.

2. **Use `pyo3-async-runtimes` (0.28) with `tokio-runtime` feature** for
   native Python `asyncio` integration. Async Rust methods return Python
   awaitables via `future_into_py`.

3. **A single shared multi-thread tokio `Runtime`** is created at module
   initialization (via `OnceLock`), replacing the per-channel
   `current_thread` runtime.

4. **Expose the `RuntimeBuilder` → `RuntimeHandle` lifecycle** as Python
   classes, matching the embedder API documented in `bindable-surface.md`.

5. **Bridge `tokio::broadcast` receivers** to both Python async iterators
   (`__aiter__`/`__anext__`) and callback registration (`on_sample()`, etc.).

6. **Expose trainer bridge methods** (`trainer_connect`, `trainer_send`,
   `trainer_recv`, `trainer_disconnect`) on `RuntimeHandle` so the Python ML
   bridge can communicate in-process via `RuntimeIpcHandle` channels.

7. **Fully rewrite the Python bridge, control, and ipc modules** to call
   Rust bindings directly — no JSON, no sockets, no `asyncio.to_thread`.

8. **Remove the old `IpcChannel` class** entirely. Python always embeds the
   runtime in-process; the external IPC server (`neurohid-ipc`) continues to
   serve Hub and CLI clients over sockets.

## Consequences

### Positive

- **Zero-copy data path:** Signal samples, features, and actions flow through
  tokio broadcast channels directly — no serialization overhead.
- **Lower latency:** In-process function calls replace socket round-trips.
- **Free-threading ready:** PyO3 0.28 + `abi3-py314` supports `Py_GIL_DISABLED`
  builds. All exposed types are `Send + Sync` (runtime uses `Arc`/tokio sync).
- **Richer Python API:** Native `RuntimeBuilder`, `RuntimeHandle`, stream
  subscriptions, and typed domain objects replace raw JSON envelopes.
- **Simpler Python code:** Bridge/control/ipc modules shrink significantly;
  protocol framing and reconnect logic disappear.

### Negative

- **Larger binary:** `neurohid-py` now links `neurohid-core` and transitive
  deps (device drivers, signal processing, platform HID layer).
- **Single-process model:** Python and the runtime share a process; a crash
  in native code takes down the Python interpreter.
- **`ipckit` stays in workspace:** `neurohid-ipc` server still needs it for
  Hub/CLI, so the workspace dependency remains.

### Neutral

- **Python ≥ 3.14 requirement unchanged** (`abi3-py314` already set).
- **`neurohid-ipc` server unchanged** — external clients (Hub, CLI) still
  connect over sockets as before.
- **Crate boundary update required** in `docs/crate-boundaries.md`.

## Alternatives Considered

| Option | Pros | Cons |
|--------|------|------|
| Keep IPC shim, add async wrappers | No architecture change | Still pays socket + JSON cost; limited API surface |
| Shared memory transport | Lower latency than sockets | Complex; still cross-process; no direct API access |
| In-process with IPC fallback | Flexibility for external mode | Maintains two code paths; test surface doubles |

## References

- `docs/bindable-surface.md` — BIND-02 specification of intended Python API surface
- `docs/crate-boundaries.md` — Crate dependency rules and layer map
- `crates/neurohid-core/src/runtime.rs` — `RuntimeBuilder`, `RuntimeHandle`, `RuntimeIpcHandle`
- `pyo3-async-runtimes` 0.28 — <https://crates.io/crates/pyo3-async-runtimes>
