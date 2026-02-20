---
phase: 02-standalone-runtime-and-control
verified: "2026-02-20T00:00:00Z"
status: passed
score: 5/5 must-haves verified
---

# Phase 2: Standalone Runtime and Control Verification Report

**Phase Goal:** User can run the decoder in the background and control it without the Hub GUI.
**Verified:** 2026-02-20
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #   | Truth   | Status     | Evidence       |
| --- | ------- | ---------- | -------------- |
| 1   | User can start the standalone runtime with a chosen profile and have it run without the Hub GUI | ✓ VERIFIED | `neurohid-service` accepts `--profile`; `run_managed_runtime(runtime, args.control_port)` starts runtime; `docs/deployment-guide.md` "Standalone runtime (without Hub)" states profile implies decoder and startup without Hub. |
| 2   | Control endpoint is reachable when running standalone (so status and output toggle work without Hub) | ✓ VERIFIED | `effective_control_port` uses `DEFAULT_STANDALONE_CONTROL_PORT` (47384) when `service_config.ipc_endpoint.trim().is_empty()` and no `--control-port`; `resolve_runtime_ipc_server_config` returns `Some(RuntimeIpcConfig)` with `127.0.0.1:47384`; `run_ipc_control_server` started with that config. |
| 3   | User can get runtime status (device connected, decoder loaded, output enabled, integrity) via control without opening the Hub | ✓ VERIFIED | `CliCommand::Control` → `run_control_command_sync`; `ControlCommandCli::Snapshot` sends `ControlCommand::Snapshot` via `send_control_request_blocking`; response `ControlResponsePayload::Snapshot { snapshot }` printed with `device_connected`, `decoder_ready`, `output_enabled`, `pipeline_integrity_degraded`, `integrity_issue_count`. |
| 4   | User can enable/disable action output via control (CLI or Hub) while runtime is running | ✓ VERIFIED | `ControlCommandCli::SetOutputEnabled { enabled }` sends `ControlCommand::SetOutputEnabled { enabled }` via `send_control_request_blocking`; handles `ControlResponsePayload::Ack` and prints `output_enabled`. |
| 5   | Developer can send control requests (snapshot, set output enabled) via CLI | ✓ VERIFIED | `neurohid-service control snapshot [--endpoint ADDR]` and `neurohid-service control set-output-enabled <true|false> [--endpoint ADDR]` with default endpoint `127.0.0.1:47384`; implemented in `run_control_command_sync` using `neurohid_ipc::send_control_request_blocking` and `neurohid_types::ControlCommand`/`ControlRequest`/`ControlResponsePayload`. |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected    | Status | Details |
| -------- | ----------- | ------ | ------- |
| `crates/neurohid/src/bin/neurohid-service.rs` | Service entrypoint; default control port; control server binding; control subcommand (snapshot, set-output-enabled) | ✓ VERIFIED | Exists; `DEFAULT_STANDALONE_CONTROL_PORT`, `effective_control_port`, `resolve_runtime_ipc_server_config`; `CliCommand::Control`, `ControlCommandCli`, `run_control_command_sync` with `send_control_request_blocking`, `ControlCommand::Snapshot`/`SetOutputEnabled`; response parsing and println for snapshot fields and ack. |
| `docs/deployment-guide.md` | Standalone startup and control endpoint docs; Control CLI usage | ✓ VERIFIED | "Standalone runtime (without Hub)" section: profile/decoder, `--config`/`--profile`/`--control-port`, default 127.0.0.1:47384, control without Hub. "Control CLI" subsection: `control snapshot` and `control set-output-enabled`, default endpoint, "without opening the Hub", "script control", link to protocol-and-api. |

### Key Link Verification

| From | To  | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| neurohid-service main flow | control server | run_managed_runtime → effective_control_port → resolve_runtime_ipc_server_config | ✓ WIRED | `main` calls `run_managed_runtime(runtime, args.control_port)`; inside it `effective_control_port` computed and passed to `resolve_runtime_ipc_server_config`; when Some, `run_ipc_control_server(server_config, ...)` started. |
| docs/deployment-guide.md | profile and control | Documented startup and --control-port / default | ✓ WIRED | Standalone section documents profile, decoder-from-profile, startup options, default 127.0.0.1:47384, and control (snapshot, set_output_enabled) without Hub. |
| control subcommand | neurohid-ipc | send_control_request_blocking | ✓ WIRED | `run_control_command_sync` calls `send_control_request_blocking(config, ControlRequest::new(...), "cli", 1)` for Snapshot and SetOutputEnabled; neurohid-ipc exports `send_control_request_blocking`. |
| control subcommand | neurohid-types ControlCommand | ControlCommand::Snapshot, SetOutputEnabled | ✓ WIRED | Uses `ControlRequest::new(ControlCommand::Snapshot)` and `ControlRequest::new(ControlCommand::SetOutputEnabled { enabled })`; parses `ControlResponsePayload::Snapshot { snapshot }` and `ControlResponsePayload::Ack`. |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| RUNT-01 | 02-01 | User can start the standalone runtime (neurohid-service or equivalent) with a chosen profile and attached decoder and have it run without the Hub GUI | ✓ SATISFIED | Service starts with `--profile`; default control port 47384 when config has no IPC endpoint; deployment guide documents profile = decoder source and standalone startup. |
| RUNT-02 | 02-02 | User can enable/disable action output (e.g. HID) via control (CLI or Hub) while the runtime is running | ✓ SATISFIED | `neurohid-service control set-output-enabled true|false` sends `SetOutputEnabled` to running service; response handled. |
| RUNT-03 | 02-02 | User can get runtime status (device connected, decoder loaded, output enabled, integrity) via control without opening the Hub | ✓ SATISFIED | `neurohid-service control snapshot` returns device_connected, decoder_ready, output_enabled, pipeline_integrity_degraded, integrity_issue_count. |
| COMP-03 | 02-02 | Developer can start/stop runtime and send control requests (e.g. snapshot, set output enabled) via SDK or CLI | ✓ SATISFIED | CLI path: `neurohid-service control snapshot` and `neurohid-service control set-output-enabled`; SDK path: neurohid-ipc `send_control_request_blocking`/`send_control_request_once` and Hub uses same API. |

All phase requirement IDs (RUNT-01, RUNT-02, RUNT-03, COMP-03) are claimed by plan frontmatter and satisfied by implementation. No orphaned requirements.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| (none) | - | No TODO/FIXME/placeholder in modified files | - | - |

### Human Verification Required

None. Automated checks confirm artifacts exist, are substantive, and are wired; control CLI and default control port behavior are implemented and documented.

### Gaps Summary

None. Phase goal is achieved: user can run the decoder in the background (neurohid-service with profile) and control it without the Hub GUI (default control endpoint 127.0.0.1:47384 and control CLI for snapshot and set-output-enabled).

---

_Verified: 2026-02-20_
_Verifier: Claude (gsd-verifier)_
