# Phase 2: Standalone runtime and control - Research

**Researched:** 2026-02-20
**Domain:** Standalone headless runtime, control plane (CLI/Hub/SDK), runtime status and output gating without Hub GUI
**Confidence:** HIGH

## Summary

Phase 2 goal is to let users run the decoder in the background and control it without the Hub GUI. The codebase already implements most of the required behavior: the `neurohid-service` binary starts with a chosen profile and config (decoder comes from the profile), runs headless, exposes an IPC v3 control endpoint when configured or when `--control-port` is used, and handles `snapshot` and `set_output_enabled` (and other commands) on the `control.rpc` channel. `ControlSnapshot` already carries device connected, decoder loaded, output enabled, and integrity fields. The SDK exposes `RuntimeBuilder`/`RuntimeHandle` for in-process start/stop and `dispatch_control_request`, and the IPC crate exposes `send_control_request_once` / `send_control_request_blocking` for external control. Gaps to close for Phase 2 are: (1) ensuring the standalone service is started with a control endpoint by default or by clear documentation so status and output toggle are reachable without the Hub; (2) providing a dedicated CLI path for “send control requests” (e.g. snapshot, set output enabled) to a running service, so COMP-03 “via SDK or CLI” is satisfied for control (daemon start/stop already exist); (3) aligning Phase 1 versioned config/profile so the service loads only supported formats. No new runtime frameworks are required; use the existing Tokio async runtime, neurohid-ipc transport, and neurohid-types control contracts.

**Primary recommendation:** Treat Phase 2 as documentation, defaults, and a small control CLI (or documented script/SDK usage). Do not re-implement the runtime or control protocol; wire defaults and a clear “control without Hub” path and document SDK/CLI for developers.

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| RUNT-01 | User can start the standalone runtime (neurohid-service or equivalent) with a chosen profile and attached decoder and have it run without the Hub GUI | Service already supports `--config`, `--profile`; decoder is loaded from profile via `RuntimeBuilder::with_profile_id`. Plan: document startup and ensure “attached decoder” is clearly “profile implies decoder”; optionally clarify CLI for decoder-from-profile. |
| RUNT-02 | User can enable/disable action output (e.g. HID) via control (CLI or Hub) while the runtime is running | `ControlCommand::SetOutputEnabled { enabled }` and `RuntimeCommand::ToggleOutput { enabled }` exist; service and Hub both dispatch it. Plan: ensure control endpoint is reachable when running standalone (default or `--control-port`) and provide CLI or doc for sending this command. |
| RUNT-03 | User can get runtime status (device connected, decoder loaded, output enabled, integrity) via control without opening the Hub | `ControlSnapshot` has `device_connected`, `decoder_ready`, `output_enabled`, `profile_ready`, `pipeline_integrity_degraded`, `integrity_issue_count`, `stage_health_summary`. Plan: ensure service exposes control (config or `--control-port`) and document/client path for snapshot without Hub. |
| COMP-03 | Developer can start/stop runtime and send control requests (e.g. snapshot, set output enabled) via SDK or CLI | SDK: `RuntimeBuilder::start()`, `RuntimeHandle::wait()`, `dispatch_control_request()`; IPC: `send_control_request_once` / `send_control_request_blocking` with `IpcConfig`. CLI: daemon start/stop via `neurohid-service daemon start|stop`. Gap: no dedicated CLI to send control to a running service (only neurohid-validate and Hub do today). Plan: add a small control CLI (e.g. `neurohid control snapshot --endpoint ...`) or document using SDK/IPC from a script. |

</phase_requirements>

## User Constraints

No CONTEXT.md was found for this phase. No locked decisions or deferred ideas to copy. Planning can proceed without user-constraint sections.

## Standard Stack

### Core

| Library / Component | Version / Location | Purpose | Why Standard |
|--------------------|--------------------|---------|--------------|
| neurohid-service binary | crates/neurohid | Headless runtime entrypoint | Already implements profile + config load, RuntimeBuilder, IPC server when endpoint set |
| neurohid-core (RuntimeBuilder, RuntimeHandle, NeuroHidService) | crates/neurohid-core | In-process runtime and control facade | Used by service and Hub; exposes start/stop and dispatch_control_request |
| neurohid-types (ControlRequest, ControlCommand, ControlSnapshot) | crates/neurohid-types | Control wire format | Single source of truth for control.rpc payloads |
| neurohid-ipc (IpcServer, IpcClient, send_control_request_once/blocking) | crates/neurohid-ipc | Control transport | Framed IPC v3 over TCP loopback or local socket; already used by Hub and validate |
| Tokio | 1.x (workspace) | Async runtime | Already used by service and IPC |

No new dependencies required. Use existing binaries, crates, and control types.

### Supporting

| Library / Component | Purpose | When to Use |
|--------------------|---------|-------------|
| clap | CLI parsing | Any new control CLI or service subcommands |
| neurohid-storage (ProfileStore, ConfigStore) | Profile and config load | Service already uses for profile + config; Phase 1 versioned formats apply |
| docs/formats (config-format.md, profile-format.md) | Versioned config/profile | Service must load only supported format versions (Phase 1 deliverable) |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| New “neurohid control” CLI | Document SDK/IPC only | CLI improves discoverability and scripting for COMP-03 |
| Custom control protocol | Keep IPC v3 control.rpc | Existing protocol and types are sufficient; no new protocol needed |

## Architecture Patterns

### Recommended Project Structure

- **Service entrypoint:** `crates/neurohid/src/bin/neurohid-service.rs` — keep as single binary; add subcommands or flags only as needed.
- **Control client for external process:** Use `neurohid_ipc::send_control_request_once` or `send_control_request_blocking` with `IpcConfig { transport, endpoint }` (e.g. `TcpLoopback`, `"127.0.0.1:47384"`).
- **SDK embedder:** Use `neurohid_core::RuntimeBuilder` and `RuntimeHandle::dispatch_control_request` for in-process control; use `neurohid_ipc` for out-of-process control to an existing service.
- **Config:** `SystemConfig.service.ipc_mode` and `ipc_endpoint` (or `--control-port` override) determine whether the service exposes the control server; empty endpoint disables it.

### Pattern 1: Start standalone service with control exposed

**What:** Run `neurohid-service` with either a config that sets `service.ipc_endpoint` (e.g. `127.0.0.1:47384` for TCP) or pass `--control-port <port>` so the service binds a control endpoint. Then any client that can reach that endpoint can send `ControlRequest { command: Snapshot }` or `SetOutputEnabled { enabled }`.

**When to use:** RUNT-02, RUNT-03, COMP-03 — “control without Hub” and “developer sends control via CLI/SDK.”

**Example (existing):**

```rust
// Service side (already in neurohid-service)
let runtime_handle = builder.start().await?;
if let Some(server_config) = resolve_runtime_ipc_server_config(&service_config, control_port)? {
    run_ipc_control_server(server_config, runtime_handle.ipc_handle(), ...).await?;
}

// Client side (e.g. Hub or a new CLI)
let config = IpcConfig { transport: IpcTransport::TcpLoopback, endpoint: "127.0.0.1:47384".into(), ... };
let response = send_control_request_blocking(config, ControlRequest::new(ControlCommand::Snapshot), "cli", 1)?;
```

### Pattern 2: SDK in-process start/stop and control

**What:** Embedder creates `RuntimeBuilder::new(config).with_profile_store(store).with_profile_id(id)`, calls `builder.start().await` to get `RuntimeHandle`, then uses `handle.dispatch_control_request(request)` for snapshot or set_output_enabled, and `handle.command(RuntimeCommand::Stop)` or `handle.wait().await` for shutdown.

**When to use:** COMP-03 “via SDK” when the runtime is in the same process.

**Example (from neurohid-core):**

```rust
let runtime = RuntimeBuilder::new(config).with_profile_store(store).with_profile_id(profile_id).start().await?;
let snapshot = runtime.dispatch_control_request(ControlRequest::new(ControlCommand::Snapshot));
runtime.command(RuntimeCommand::Stop)?;
runtime.wait().await?;
```

### Anti-Patterns to Avoid

- **Adding a second control protocol:** Keep a single control surface (control.rpc over IPC v3). Do not introduce a separate REST or gRPC control API for this phase.
- **Requiring the Hub for status or output toggle:** Phase 2 success criteria require control “without opening the Hub.” Ensure default or one-flag startup exposes control (e.g. default TCP port or documented `--control-port`).
- **Hand-rolling a new daemon or service host:** Use the existing `neurohid-service` and, on Windows, existing Windows service integration; do not replace the binary.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Control wire format | New JSON schema or binary protocol | neurohid-types `ControlRequest` / `ControlResponse` / `ControlSnapshot` | Already versioned and used by Hub, validate, and service |
| Transport for control | New socket or HTTP server | neurohid-ipc `IpcServer` / `IpcClient` and framed IPC v3 | Local-only, same envelope as trainer/runtime.events |
| Runtime lifecycle | New process manager or supervisor | Existing `neurohid-service` + `RuntimeBuilder`/`NeuroHidService` | Already handles profile, config, decoder load, and IPC server |
| Status snapshot fields | New “status” DTO | `ControlSnapshot` | Already includes device_connected, decoder_ready, output_enabled, integrity fields |

**Key insight:** Phase 2 is primarily integration and documentation. The runtime and control types already exist; the planner should add a minimal control CLI (or documented SDK/script path) and ensure the service exposes control by default or via a single, documented option.

## Common Pitfalls

### Pitfall 1: Service runs without a control endpoint

**What goes wrong:** User starts `neurohid-service` and cannot get status or toggle output because no client can connect (config has empty `ipc_endpoint` and user did not pass `--control-port`).

**Why it happens:** `resolve_runtime_ipc_server_config` returns `None` when the endpoint is empty; the IPC server is not started.

**How to avoid:** Document that for “control without Hub” the user must either set `service.ipc_endpoint` (e.g. `127.0.0.1:47384`) and `service.ipc_mode = "tcp_loopback"` in config, or pass `--control-port <port>`. Consider defaulting to a TCP port when running as standalone (e.g. 47384) if config endpoint is empty, so control is always available when intended.

**Warning signs:** Deployment guide or phase docs do not state how to enable the control endpoint for standalone runs.

### Pitfall 2: No CLI to send control requests

**What goes wrong:** COMP-03 says “via SDK or CLI”; developers have no obvious CLI to send snapshot or set_output_enabled to a running service (only neurohid-validate and Hub do today).

**Why it happens:** Control was implemented for Hub and tests; no standalone “neurohid control” command was added.

**How to avoid:** Add a small CLI (e.g. `neurohid control snapshot --endpoint 127.0.0.1:47384`, `neurohid control set-output-enabled true/false`) or document a minimal script using `send_control_request_blocking` (or a tiny example binary). Prefer a dedicated subcommand or binary so COMP-03 is clearly satisfied for CLI.

**Warning signs:** COMP-03 verification asks “how does a developer send control via CLI?” and the only answer is “use neurohid-validate” or “call SDK from code.”

### Pitfall 3: Decoder “attached” vs profile unclear

**What goes wrong:** RUNT-01 says “with a chosen profile and attached decoder”; implementers or users assume a separate decoder selection path, or expect a decoder to be optional when a profile is set.

**Why it happens:** Today the decoder is loaded from the active profile (profile implies decoder). There is no separate “attach decoder” CLI flag.

**How to avoid:** In Phase 2 docs and acceptance criteria, define “attached decoder” as “decoder loaded from the chosen profile” (current behavior). Document that the user selects profile; the service loads the decoder associated with that profile. No separate decoder attachment step unless the product later adds it.

**Warning signs:** Plan introduces a new “decoder path” or “attach decoder” flow that duplicates profile-based loading.

## Code Examples

### Sending a snapshot request to a running service (external client)

```rust
// Source: crates/neurohid-ipc/src/client.rs, neurohid-service daemon status
use neurohid_ipc::{send_control_request_blocking, IpcConfig, IpcTransport};
use neurohid_types::control::{ControlCommand, ControlRequest};

let config = IpcConfig {
    transport: IpcTransport::TcpLoopback,
    endpoint: "127.0.0.1:47384".to_string(),
    ..Default::default()
};
let response = send_control_request_blocking(
    config,
    ControlRequest::new(ControlCommand::Snapshot),
    "cli-session",
    1,
)?;
if let ControlResponsePayload::Snapshot { snapshot } = response.payload {
    println!("output_enabled={} device_connected={} decoder_ready={}", 
             snapshot.output_enabled, snapshot.device_connected, snapshot.decoder_ready);
}
```

### Service startup with profile and config (existing)

```bash
# From docs/deployment-guide.md and neurohid-service --help
cargo run -p neurohid --bin neurohid-service -- --config /path/to/config.toml --profile my-profile --control-port 47384
# Or daemon
cargo run -p neurohid --bin neurohid-service -- daemon start --config /path/to/config.toml --profile my-profile --control-port 47384
```

### SDK in-process: start runtime and send control

```rust
// Source: crates/neurohid-core/src/runtime.rs (tests)
let runtime = RuntimeBuilder::new(config)
    .with_profile_store(profile_store)
    .with_profile_id(profile_id)
    .start()
    .await?;
let snap = runtime.dispatch_control_request(ControlRequest::new(ControlCommand::Snapshot));
runtime.dispatch_control_request(ControlRequest {
    request_id: None,
    command: ControlCommand::SetOutputEnabled { enabled: false },
});
runtime.command(RuntimeCommand::Stop)?;
runtime.wait().await?;
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Hub-only control | control.rpc over IPC v3; Hub and service both use it | Existing codebase | Standalone service can serve control; need default endpoint or doc |
| No formal “standalone” criteria | Phase 2 success criteria (run without Hub, control without Hub, status without Hub, SDK/CLI) | This phase | Plan must close gaps (default control endpoint, control CLI or doc) |
| Unversioned config/profile | Phase 1 format_version + compatibility | Phase 1 | Service should load only supported format versions; no change to control types |

**Deprecated/outdated:** Assuming control is only for the Hub; Phase 2 requires control to be usable without the Hub.

## Open Questions

1. **Default control endpoint for standalone**
   - What we know: If `service.ipc_endpoint` is non-empty, the server starts; `--control-port` overrides to TCP. Default endpoint in types is `"neurohid.control.v3"` (local socket name).
   - What's unclear: Whether standalone runs should default to a TCP port (e.g. 47384) when no config is provided, so control is always available without editing config.
   - Recommendation: Plan either (a) document that users must set config or use `--control-port`, or (b) add a default TCP port when running as standalone (e.g. if no config file or explicit endpoint).

2. **Control CLI shape**
   - What we know: COMP-03 requires “via SDK or CLI.” SDK path is clear; CLI for “send control” is missing.
   - What's unclear: Subcommand under `neurohid` (e.g. `neurohid control snapshot`) vs separate binary vs only documentation + example.
   - Recommendation: Prefer a `neurohid control` subcommand (or equivalent) that accepts endpoint and command (snapshot, set-output-enabled, etc.) so COMP-03 is verifiable and scripting is simple.

3. **Windows service and control endpoint**
   - What we know: Windows service install/start/stop exist; service reads config from storage; control_port can be passed at install time only if persisted.
   - What's unclear: How Windows service users get a stable control endpoint (e.g. fixed port in config) for RUNT-02/RUNT-03.
   - Recommendation: Document that for Windows service, config should set `service.ipc_endpoint` (e.g. `127.0.0.1:47384`) so control is reachable after service start; no code change required if config is already loaded.

## Sources

### Primary (HIGH confidence)

- Codebase: `crates/neurohid/src/bin/neurohid-service.rs` (Args, load_runtime_context, run_managed_runtime, resolve_runtime_ipc_server_config, run_ipc_control_server, daemon commands); `crates/neurohid-core/src/runtime.rs` (RuntimeBuilder, RuntimeHandle, dispatch_control_request, command); `crates/neurohid-core/src/service.rs` (NeuroHidService, set_output_enabled); `crates/neurohid-types/src/control.rs` (ControlSnapshot, ControlCommand); `crates/neurohid-ipc/src/client.rs` (send_control_request_once, send_control_request_blocking); `docs/protocol-and-api.md`, `docs/integration-architecture.md`, `docs/deployment-guide.md`
- Requirements: `.planning/REQUIREMENTS.md` (RUNT-01, RUNT-02, RUNT-03, COMP-03)
- Phase 1: `.planning/phases/01-contracts-and-versioned-formats/01-RESEARCH.md` (versioned config/profile; service will load these)

### Secondary (MEDIUM confidence)

- Project state: `.planning/STATE.md`, `.planning/PROJECT.md` (current focus, decisions)
- Stack: `.planning/codebase/STACK.md` (Tokio, ipckit, clap, windows-service)

### Tertiary (LOW confidence)

- None

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — existing binaries and crates; no new libraries.
- Architecture: HIGH — control and service flow are implemented; gaps are endpoint defaults and control CLI.
- Pitfalls: HIGH — codebase and deployment docs confirm endpoint and CLI gaps.

**Research date:** 2026-02-20  
**Valid until:** ~30 days (runtime and control contracts are stable; only defaults and CLI shape may be refined in planning).

## RESEARCH COMPLETE
