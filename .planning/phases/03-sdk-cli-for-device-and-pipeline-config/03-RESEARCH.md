# Phase 3: SDK/CLI for device and pipeline config - Research

**Researched:** 2026-02-20
**Domain:** Public SDK and CLI for device discovery/connection/stream selection and signal/decoder pipeline configuration
**Confidence:** HIGH

## Summary

Phase 3 adds developer-facing APIs and CLI so that device discovery, connection, stream selection, and pipeline/decoder configuration can be driven without the Hub. The codebase already provides the building blocks: device discovery and connect/disconnect live in `neurohid-core` (DeviceTask, `DeviceCommand::Rescan`/`Connect`/`Disconnect`), exposed via `RuntimeCommand` and `ControlCommand`; `ControlSnapshot` includes `discovered_streams` and pipeline status; `RuntimeHandle` and IPC control clients allow in-process and out-of-process control. Config is versioned TOML in `neurohid-types` (`SystemConfig`, `DecoderConfig`, `SignalConfig`) and loaded by `neurohid-storage::ConfigStore` (TOML only today). The SDK crate is a feature-gated re-export facade with no high-level device/pipeline API yet. Gaps to close: (1) a stable SDK surface for discovery (list, connect-by-id, connect-by-criteria), ongoing discovery/callbacks, and handle lifecycle; (2) a CLI with subcommands per concern (`device list`/`device connect`, `config set`/`config validate`, `pipeline ...`) and the output/exit-code/flag conventions from CONTEXT.md; (3) support for both YAML and TOML config files and a documented pipeline/decoder config format. Use the existing stack (clap 4.4, Tokio, neurohid-types control/config, neurohid-core runtime) and extend rather than replace.

**Primary recommendation:** Expose device discovery and connection via the SDK as a thin facade over the existing runtime (snapshot for list, `RuntimeCommand::RescanStreams`/`ConnectStream`/`DisconnectStream`) plus an optional discovery-only path using `DeviceProvider::discover()` for scripts that need to list without starting the full runtime. Add a unified CLI (subcommands under one binary, e.g. `neurohid` with CLI mode or a dedicated `neurohid-cli`) implementing the CONTEXT.md conventions. Add YAML config support in storage layer and document decoder/signal config scope in the existing config format doc.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **Discovery and connection flow:** List + connect-by-id and connect-by-criteria; ongoing discovery in background; SDK exposes updating list or notifies when devices appear/disappear; optional listener/callback for reactive UIs plus current device list for scripts; first match when connecting by criteria (order implementation-defined); multiple simultaneous connections in scope; on device disappear SDK notifies and can invalidate handle; handles ref-counted or scoped (drop = disconnect), explicit disconnect optional.
- **CLI shape and output:** Subcommands per concern (`neurohid device list`, `neurohid device connect`, `neurohid config set`, `neurohid pipeline ...`); optional Hydra-style config for composition/overrides; default human-readable table, `--json` for scriptable output; default normal, `-v`/`--verbose` for more, `-q`/`--quiet` for less; config file as main source, flags override (`--config`, `--profile`); exit codes 0 = success, non-zero = failure, document codes (e.g. 1 = generic, 2 = not found, 3 = config invalid); progress on stderr, result on stdout, `-q` suppresses progress; short one-line summary per subcommand in main help, full description in man/docs or `neurohid <cmd> --help`; interactive prompt when ambiguous (e.g. multiple devices and no `--device-id`), otherwise no prompts for scripts/CI; compact one-line JSON by default for `--json`, optional `--json pretty` or `-v` for indented; dry run / validate (e.g. `neurohid pipeline run --dry-run`, `neurohid config validate`).
- **Config format and scope:** Pipeline/decoder scope: decoder (model path, params) plus signal preprocessing options where the stack exposes them; config file format: support both YAML and TOML.
- **Errors and status:** When `--json`, failures write JSON object (e.g. stderr) with code, message, optional details; status surface richer than minimum: pipeline-stage health (signal path ok, decoder output rate) where supported; extend or align with Phase 2 runtime status as needed.

### Claude's Discretion

- **Discovery/connection:** Sync vs async for connect; stream selection approach (list then choose vs type-based first match); whether stream selection is part of connect or separate step; device identity (stable id + name, id canonical for API); whether connection handle represents device-only or device+stream.
- **CLI:** Whether CLI is thin wrapper vs convenience flows; global vs per-command flags (e.g. `--config` and `-v`/`-q` global; `--json` only on commands that produce list/object).
- **Config:** Where config lives (one file with sections vs profile + overlay); layering (e.g. file + flags only for this phase).
- **Errors:** SDK error style (align with existing crates); CLI error presentation (single-line stderr by default vs multi-line with suggestion when `-v`).

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within phase scope.

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| COMP-01 | Developer can drive device discovery, connection, and stream selection via public SDK API (Rust) and/or CLI | Runtime already supports RescanStreams, ConnectStream, DisconnectStream via RuntimeHandle and ControlCommand; ControlSnapshot.discovered_streams is the list. SDK: expose list (from snapshot or from DeviceProvider::discover()), connect_by_id(stream_id), connect_by_criteria(predicate), optional listener for changes; CLI: `neurohid device list` (--json), `neurohid device connect [--device-id \| --criteria]`. Discovery-only path without full runtime: use neurohid-device DeviceProvider::discover() and map to DiscoveredStream-like type for scripts. |
| COMP-02 | Developer can configure signal pipeline and decoder (e.g. model path, params) via SDK/CLI and documented config format | SystemConfig contains DecoderConfig (model_path, learning params) and SignalConfig (filters, feature window, artifact rejection). ConfigStore loads/saves SystemConfig (TOML only). Plan: add YAML support (serde_yaml + format detection or --format); document pipeline/decoder section in config-format.md; SDK: get/set config or load/save via ConfigStore; CLI: `neurohid config set`, `neurohid config validate`, `neurohid config show`; pipeline subcommands for run/validate with config. |

</phase_requirements>

## Standard Stack

### Core

| Library / Component | Version / Location | Purpose | Why Standard |
|--------------------|--------------------|---------|--------------|
| clap | 4.4 (derive) | CLI parsing | Already used in neurohid/neurohid-service; subcommands, --json, -v/-q are straightforward |
| neurohid-types | crates/neurohid-types | DeviceInfo, DiscoveredStream, ControlSnapshot, SystemConfig, DecoderConfig, SignalConfig, ControlCommand | Single source of truth for wire and config types |
| neurohid-core | crates/neurohid-core | RuntimeBuilder, RuntimeHandle, RuntimeCommand, DeviceCommand, NeuroHidService | Discovery/connect implemented here; SDK facade should delegate here |
| neurohid-device | crates/neurohid-device | DeviceProvider::discover(), DeviceInfo | For discovery-only SDK path without starting runtime |
| neurohid-storage | crates/neurohid-storage | ConfigStore, ProfileStore, DataPaths | Config load/save; extend for YAML |
| neurohid-ipc | crates/neurohid-ipc | send_control_request_once, IpcConfig | Out-of-process control for CLI talking to running service |
| neurohid-sdk | crates/neurohid-sdk | Public facade | Re-export + new high-level device/config API surface |

### Supporting

| Library / Component | Purpose | When to Use |
|--------------------|---------|-------------|
| serde_yaml | YAML parse/serialize | Add to neurohid-storage (or neurohid-types) for config; detect by extension (.yaml/.yml) or explicit flag |
| toml | TOML (existing) | Keep as default; ConfigStore already uses it |
| docs/formats/config-format.md | Versioned config schema | Extend with pipeline/decoder scope and YAML note |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| New SDK facade over runtime | Direct use of neurohid-core only | Facade keeps API stable and documents intended developer surface |
| Single neurohid-service binary for all CLI | Separate neurohid CLI binary | One binary with subcommands (device/config/pipeline) improves UX; can be neurohid with "no GUI" mode or new binary name |
| YAML + TOML | TOML only | CONTEXT mandates both; add serde_yaml and format detection |

**Installation (for new dependency):**

```bash
# In crates/neurohid-storage or workspace Cargo.toml
cargo add serde_yaml
```

## Architecture Patterns

### Recommended Project Structure

- **SDK device API:** Live in `neurohid-sdk` (or a new `neurohid-sdk::device` module re-exporting and wrapping). Option A: expose `RuntimeHandle`-centric API (list = snapshot after rescan, connect/disconnect = RuntimeCommand). Option B: expose a `DiscoverySession` that uses `DeviceProvider::discover()` and optionally attaches to a runtime for connect. CONTEXT allows "list + connect-by-id and connect-by-criteria"; "ongoing discovery" and "listener/callback" — runtime already has background discovery when device_command_rx is present; snapshot is the "current list."
- **CLI:** One entrypoint with subcommands: `device`, `config`, `pipeline`, and optionally `control` (Phase 2 already has control under neurohid-service). Either extend `neurohid` binary to accept subcommands and run CLI when first arg is a command, or add `neurohid-cli` / use `neurohid-service` for both daemon and CLI (e.g. `neurohid-service device list`). CONTEXT says "neurohid device list" — so command name is `neurohid`; implement as `neurohid` binary that dispatches to CLI vs Hub by args.
- **Config:** One file with sections (SystemConfig); config file path from `--config` or DataPaths::config_file(); profile from `--profile` or default. Layering: file + flags for this phase (no Hydra required in v1, optional later).

### Pattern 1: List devices via runtime snapshot

**What:** Start runtime (or connect to existing service via IPC), send RescanStreams then Snapshot, read `snapshot.discovered_streams` for the list. For CLI talking to running service: use neurohid-ipc `send_control_request_once` with RescanStreams then Snapshot.

**When to use:** COMP-01 "list" when a runtime or service is already running.

**Example (existing runtime):**

```rust
// In-process
runtime.command(RuntimeCommand::RescanStreams)?;
let snap = runtime.dispatch_control_request(ControlRequest::new(ControlCommand::Snapshot));
let streams = snap.payload.snapshot().discovered_streams;
```

### Pattern 2: Connect by id / by criteria

**What:** By id: send `ConnectStream { stream_id }`. By criteria: list streams (snapshot or discover()), filter (e.g. first with type "EEG"), then ConnectStream with that id. Document that order is implementation-defined.

**When to use:** COMP-01 connect-by-id and connect-by-criteria.

### Pattern 3: Discovery without full runtime

**What:** Use `DeviceProvider::discover()` (e.g. LslProvider, MockProvider) to get `Vec<DeviceInfo>`, then map to a stable id/name list for scripts. No need to start decoder/signal pipeline. SDK can expose this as a lightweight "list available devices/streams" API.

**When to use:** Scripts that only need to enumerate; CI; connect-by-criteria when runtime not yet started (choose stream, then start runtime with that stream id or pass to service).

### Pattern 4: Config load/save with YAML and TOML

**What:** ConfigStore today: `toml::from_str` / `toml::to_string_pretty`. Add format detection by path extension (`.yaml`/`.yml` => serde_yaml) or explicit `format` parameter; keep TOML default. Same SystemConfig schema for both.

**When to use:** COMP-02 "documented config format" and "support both YAML and TOML."

### Anti-Patterns to Avoid

- **Don't duplicate discovery logic:** Use DeviceTask/Provider and RuntimeCommand/ControlCommand instead of reimplementing discovery in the SDK.
- **Don't skip versioned config:** Config load must respect format_version and compatibility policy (docs/formats/config-format.md).
- **Don't prompt in non-interactive mode:** When args are explicit (e.g. --device-id), no prompts; prompt only when ambiguous and TTY.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|--------------|-----|
| CLI parsing | Manual args | clap 4.4 derive, Subcommand | Subcommands, --json, -v/-q, help are standard |
| Config serialization | Custom YAML/TOML | serde + toml + serde_yaml | Schema lives in neurohid-types; one source of truth |
| Device discovery | Custom discovery | DeviceProvider::discover(), or runtime RescanStreams + Snapshot | Backends (LSL, Serial, Mock) already implemented |
| Control protocol | New wire format | ControlRequest/ControlResponse, neurohid-ipc | Phase 2 already defined and used |
| Exit codes | Ad-hoc | Documented convention (0, 1, 2, 3) | Scripts and CI rely on stable codes |

**Key insight:** The runtime and control plane already implement discovery and connection; the phase is about exposing them through a stable SDK and a consistent CLI, and adding YAML + docs for config.

## Common Pitfalls

### Pitfall 1: Discovery lifecycle vs runtime lifecycle

**What goes wrong:** SDK exposes "list devices" but caller expects list to update without holding a runtime; or runtime is stopped and list handle becomes stale.

**Why it happens:** Confusion between "discovery-only" (Provider::discover()) and "runtime-owned list" (snapshot.discovered_streams).

**How to avoid:** Document clearly: (1) discovery-only API returns a point-in-time list and does not run background discovery; (2) runtime-based list is live and updates when RescanStreams runs and when streams connect/disconnect; (3) handle invalidation on disconnect (CONTEXT: "when device disappears SDK notifies and can invalidate handle").

**Warning signs:** Tests that assume list length without rescan; docs that say "list updates" without saying how (callback vs poll).

### Pitfall 2: Config format and version

**What goes wrong:** CLI or SDK writes YAML that omits format_version or uses a different schema, and readers fail or misinterpret.

**Why it happens:** YAML added without applying same versioning rules as TOML (format_version at root, compatibility policy).

**How to avoid:** Use same SystemConfig and format_version for both YAML and TOML; in ConfigStore, always set format_version when saving; document in config-format.md that YAML and TOML share the same schema and version.

**Warning signs:** New code paths that serialize config without including format_version.

### Pitfall 3: CLI output and scripts

**What goes wrong:** Human-readable output mixed with JSON; progress on stdout; exit code 0 on partial failure.

**Why it happens:** Forgetting CONTEXT rules: progress on stderr, result on stdout; --json for machine output; strict exit codes.

**How to avoid:** Centralize output helpers: stdout for result only when not --json; stderr for progress and errors; when --json, errors as JSON object to stderr and non-zero exit. Document exit codes (e.g. 1 generic, 2 not found, 3 config invalid).

**Warning signs:** Println for both progress and result; exit(0) after error.

### Pitfall 4: Connect-by-criteria ordering

**What goes wrong:** Users assume a specific order (e.g. "first LSL stream by name") and behavior changes after a library update.

**Why it happens:** CONTEXT says "first match; document that order is implementation-defined."

**How to avoid:** Document clearly that connect-by-criteria returns the first match and order is not guaranteed; prefer connect-by-id for scripts when id is known.

**Warning signs:** Tests that depend on exact order of discovered streams without explicit id.

## Code Examples

### Device list from running service (IPC)

```rust
// CLI or script: service already running with control endpoint
let config = IpcConfig { transport: IpcTransport::TcpLoopback, endpoint: "127.0.0.1:47384".into(), ... };
let _ = send_control_request_blocking(config.clone(), ControlRequest::new(ControlCommand::RescanStreams), "cli", 1)?;
let response = send_control_request_blocking(config, ControlRequest::new(ControlCommand::Snapshot), "cli", 1)?;
if let ControlResponsePayload::Snapshot { snapshot } = response.payload {
    for stream in &snapshot.discovered_streams {
        println!("{} {}", stream.id, stream.name);
    }
}
```

### Connect stream (in-process runtime)

```rust
runtime.command(RuntimeCommand::ConnectStream { stream_id: "LSL-EEG-123".into() })?;
```

### Config load (existing ConfigStore, TOML)

```rust
let store = ConfigStore::new(paths);
let config = store.load().await?;
// config.decoder.model_path, config.signal.bandpass_high_hz, etc.
```

### Decoder and signal config scope (neurohid-types)

`DecoderConfig`: model_path, online_learning_enabled, learning_rate, gamma, gae_lambda, update_frequency_steps, batch_size, entropy_coef, value_coef, max_grad_norm.

`SignalConfig`: buffer_size_samples, notch_filter_enabled/hz, bandpass_*_hz, feature_window_ms, feature_step_ms, artifact_rejection_enabled, artifact_threshold_uv.

## State of the Art

| Area | Current State | Phase 3 Target |
|------|----------------|----------------|
| Config file format | TOML only (ConfigStore) | TOML + YAML; same schema, format_version |
| Device list API | Via runtime snapshot or Hub-only | Public SDK list (runtime or discovery-only) + CLI `device list` |
| Connect/disconnect | RuntimeCommand + ControlCommand | Same; SDK and CLI expose them |
| Pipeline/decoder config | SystemConfig in code, config-format.md version/schema | Document decoder/signal scope; config set/validate in CLI |
| CLI | neurohid-service (daemon + control subcommands) | Add device, config, pipeline subcommands; one entrypoint named `neurohid` for CLI surface |

**Deprecated/outdated:** None identified. Phase 2 control CLI is the baseline to extend.

## Open Questions

1. **Single binary vs two for CLI**
   - What we know: CONTEXT says "neurohid device list" (command name neurohid). Current `neurohid` binary is Hub GUI; `neurohid-service` is daemon + control.
   - What's unclear: Whether to add CLI mode to `neurohid` (e.g. `neurohid device list` → CLI, `neurohid` with no args → Hub) or ship a separate binary and document `neurohid-cli` or alias.
   - Recommendation: Prefer one binary `neurohid`: if first arg is a subcommand (device, config, pipeline, control), run CLI; otherwise launch Hub. Keeps one entrypoint for "neurohid" and satisfies CONTEXT naming.

2. **SDK sync vs async for connect**
   - What we know: RuntimeCommand::ConnectStream is fire-and-forget (try_send to device task); actual connection is async inside DeviceTask.
   - What's unclear: Whether SDK should expose `async fn connect(&self, stream_id: &str) -> Result<()>` or blocking "send command and return" (current behavior).
   - Recommendation: Document that "connect" means "command sent"; connection success can be observed via snapshot or listener. If we add async API, it can wait until snapshot shows stream.connected for that id (with timeout).

3. **Connection handle: device-only vs device+stream**
   - What we know: Today one logical "connection" is one stream (DiscoveredStream); multiple streams can be connected.
   - What's unclear: Whether SDK exposes a "ConnectionHandle" that is ref-counted and represents one stream, or just stream_id in API.
   - Recommendation: CONTEXT says "handles ref-counted or scoped (drop = disconnect)". So SDK could expose a handle that wraps stream_id and on drop sends DisconnectStream. Implementation: handle holds RuntimeIpcHandle (or RuntimeHandle) + stream_id; Drop calls DisconnectStream. That gives device+stream semantics per handle.

## Sources

### Primary (HIGH confidence)

- Codebase: crates/neurohid-core (runtime.rs, tasks/device.rs), crates/neurohid-types (control.rs, config.rs, device.rs), crates/neurohid-storage (config.rs), crates/neurohid (neurohid-service.rs), crates/neurohid-sdk (lib.rs), docs/formats/config-format.md, docs/crate-boundaries.md, .planning/codebase/STACK.md
- Phase 2 RESEARCH: .planning/phases/02-standalone-runtime-and-control/02-RESEARCH.md — control CLI and SDK patterns
- CONTEXT.md: .planning/phases/03-sdk-cli-for-device-and-pipeline-config/03-CONTEXT.md — locked decisions and discretion

### Secondary (MEDIUM confidence)

- AGENTS.md, crates/AGENTS.md — Rust lane and crate boundaries; SDK lives in neurohid-sdk, device in neurohid-device, orchestration in neurohid-core
- REQUIREMENTS.md — COMP-01, COMP-02 wording and phase mapping

### Tertiary (LOW confidence)

- No external web or Context7 lookup performed; stack and patterns are from repo and Phase 2 research only. clap 4.4 and serde_yaml are standard; versions in Cargo.toml and ecosystem norms.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — from workspace Cargo.toml and existing Phase 2 research; only addition is serde_yaml for YAML.
- Architecture: HIGH — runtime and control types already implement discovery/connect; SDK/CLI are facades and conventions.
- Pitfalls: MEDIUM — inferred from CONTEXT and common CLI/SDK issues; not validated by external sources.

**Research date:** 2026-02-20
**Valid until:** ~30 days (stable domain; config format and CLI conventions may be refined during planning).
