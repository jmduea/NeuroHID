---
phase: 03-sdk-cli-for-device-and-pipeline-config
verified: "2026-02-20"
status: passed
score: 2/2 requirements verified
---

# Phase 3: SDK/CLI for Device and Pipeline Config Verification Report

**Phase Goal:** Developer can drive device discovery and configure the signal/decoder pipeline via public API and CLI.
**Verified:** 2026-02-20
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Success Criteria (ROADMAP)

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | Developer can drive device discovery, connection, and stream selection via public SDK API (Rust) and/or CLI | ✓ VERIFIED | neurohid-sdk `device` module: `list_streams_via_runtime`, `list_streams_discovery`, `connect_by_id`, `connect_by_criteria`, `StreamConnectionHandle` (drop = disconnect). neurohid-service `DeviceCommandCli::List` and `Connect`; neurohid binary dispatches to neurohid-service for device\|config\|pipeline\|control\|daemon. |
| 2 | Developer can configure signal pipeline and decoder (e.g. model path, params) via SDK/CLI and documented config format | ✓ VERIFIED | ConfigStore YAML/TOML via path extension; `load_from_path`/`save_to_path`. neurohid-sdk `config::load`/`config::save`. CLI `config show`, `config validate`; `pipeline run --dry-run`. docs/formats/config-format.md documents DecoderConfig and SignalConfig scope and YAML/TOML parity. |

**Score:** 2/2 criteria verified

### Plan 03-01 Must-Haves (COMP-01)

| Truth / Artifact | Status | Details |
|------------------|--------|---------|
| List discovered devices/streams via SDK | ✓ | `list_streams_via_runtime`, `list_streams_discovery` in device/api.rs |
| Connect by stream id and by criteria via SDK | ✓ | `connect_by_id`, `connect_by_criteria` in device/api.rs |
| List devices via CLI (neurohid device list) human or JSON | ✓ | DeviceCommandCli::List with --json, table output; neurohid dispatches to neurohid-service |
| Connect via CLI (--device-id \| --criteria) | ✓ | DeviceCommandCli::Connect; exit 2 when stream not found |
| Handle lifecycle documented (drop = disconnect) | ✓ | StreamConnectionHandle impl Drop sends DisconnectStream; module doc in device/mod.rs |
| device.rs provides public device API | ✓ | device/mod.rs + device/api.rs, min_lines satisfied |
| neurohid-service device list subcommand | ✓ | Contains "device list"; run_device_command_sync |
| SDK → RuntimeCommand/ControlCommand (RescanStreams, ConnectStream, discovered_streams) | ✓ | list_streams_via_runtime uses Snapshot/discovered_streams; connect_by_id sends ConnectStream |
| neurohid binary → neurohid-service for device/config/pipeline/control/daemon | ✓ | neurohid.rs maybe_dispatch_to_service, CLI_SUBCOMMANDS |

### Plan 03-02 Must-Haves (COMP-02)

| Truth / Artifact | Status | Details |
|------------------|--------|---------|
| Config YAML or TOML with same schema and format_version | ✓ | neurohid-storage config.rs: is_yaml_path, load_from_path/save_to_path with serde_yaml; config-format.md documents both |
| Pipeline/decoder and signal scope documented | ✓ | config-format.md "Pipeline and decoder config scope" with DecoderConfig and SignalConfig tables |
| Load/save config via SDK | ✓ | neurohid-sdk config.rs: load/save using ConfigStore, optional path |
| CLI config validate (exit 0 valid) | ✓ | ConfigCommandCli::Validate; exit 3 invalid; --json error to stderr |
| CLI config show | ✓ | ConfigCommandCli::Show, human or --json |
| Pipeline run dry-run/validate | ✓ | PipelineCommandCli::Run { dry_run }; pipeline run --dry-run loads config, exit 0 if valid |
| neurohid-storage config.rs YAML/TOML | ✓ | serde_yaml, format by extension |
| docs/formats/config-format.md decoder/signal scope | ✓ | DecoderConfig and SignalConfig field tables |
| neurohid-sdk config.rs public API | ✓ | load/save, min_lines satisfied |
| ConfigStore → SystemConfig, format_version | ✓ | load_from_path/save_to_path use SystemConfig; format_version in schema |
| CLI config validate → ConfigStore::load | ✓ | run_config_command uses ConfigStore::load_from_path for --config |

### Requirements Coverage

| Requirement | Plan | Description | Status |
|-------------|------|-------------|--------|
| COMP-01 | 03-01 | Developer can drive device discovery, connection, and stream selection via public SDK API (Rust) and/or CLI | ✓ SATISFIED |
| COMP-02 | 03-02 | Developer can configure signal pipeline and decoder via SDK/CLI and documented config format | ✓ SATISFIED |

All phase requirement IDs (COMP-01, COMP-02) are satisfied. No gaps.

### Human Verification Required

None. Artifacts and CLI subcommands are present and wired; config format and scope are documented.

### Gaps Summary

None. Phase goal achieved: developer can drive device discovery/connection and configure pipeline/decoder via SDK and CLI.

---
_Verified: 2026-02-20_
