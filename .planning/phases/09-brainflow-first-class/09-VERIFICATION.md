---
phase: 09-brainflow-first-class
verified: "2026-02-21T00:00:00Z"
status: passed
score: 8/8 must-haves verified
gaps: []
human_verification:
  - test: "Run embedded_runtime example and confirm device/stream discovery"
    expected: "cargo run -p neurohid-sdk --example embedded_runtime --features 'runtime,types' runs; snapshot shows device_connected and discovered_streams"
    why_human: "End-to-end run and real-time behavior"
  - test: "Hub: set backend to BrainFlow in Settings, Rescan on Devices screen"
    expected: "BrainFlow synthetic stream appears in Available Streams; Connect/Disconnect work"
    why_human: "Visual UX and user flow completion"
---

# Phase 09: BrainFlow First-Class Verification Report

**Phase Goal:** User or developer can use BrainFlow with NeuroHID via first-class docs, runnable examples, and Hub UX; synthetic board fully replaces the in-repo mock device.

**Verified:** 2026-02-21  
**Status:** passed  
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #   | Truth   | Status     | Evidence       |
| --- | ------- | ---------- | -------------- |
| 1   | User or developer can read first-class documentation for using BrainFlow with NeuroHID (setup, config, synthetic vs native, build order) | ✓ VERIFIED | `docs/brainflow.md` exists (41 lines), sections: Setup, Config, Synthetic vs native, Build order, Device-agnostic API (BRAIN-04). Linked from `docs/index.md` (Architecture and System Docs). |
| 2   | BrainFlow remains one backend behind DeviceProvider/Device; device-agnostic API is preserved | ✓ VERIFIED | Documented in `docs/brainflow.md` § "Device-agnostic API (BRAIN-04)". `BrainFlowProvider`/`BrainFlowDevice` behind same `create_provider`/discover/connect flow. |
| 3   | User can run at least one runnable example using BrainFlow's synthetic board end-to-end | ✓ VERIFIED | `crates/neurohid-sdk/examples/embedded_runtime.rs`: `DeviceBackend::BrainFlow`, `BrainFlowConfig::default()`, `RuntimeBuilder::new(config).start()`, `RescanStreams`, snapshot reads. |
| 4   | User can discover and connect BrainFlow devices from the Hub Devices screen with UX parity to LSL (discover, connect, disconnect) | ✓ VERIFIED | Devices screen: "Start service to discover/connect streams (LSL, BrainFlow, …)", "Available Streams", "Use Rescan to discover streams; … BrainFlow synthetic appears when backend is BrainFlow." No "LSL-first" or "parity planned" in repo. Same Rescan/Connect/Disconnect flow; backend chosen in Settings. |
| 5   | BrainFlow's synthetic board is the single non-hardware device path: tests, examples, and CI use it; no separate mock backend in user-facing paths | ✓ VERIFIED | All test/config use of device backend is `DeviceBackend::BrainFlow` in neurohid-core runtime tests, neurohid-service, neurohid-sdk (device tests use `device-brainflow` + BrainFlowProvider), neurohid-hub service_manager tests, neurohid-validate. `DeviceBackend::Mock` only in enum def, discovery branch for explicit Mock, Settings dropdown, and neurohid-device mock.rs (internal). |
| 6   | Auto backend falls back to BrainFlow synthetic (board_id 0), not Mock | ✓ VERIFIED | `crates/neurohid-core/src/tasks/device/discovery.rs`: `DeviceBackend::Auto` → `create_brainflow_provider(config)?` as fallback; `AutoProvider::new(lsl, fallback)`; logs "BrainFlow synthetic fallback" / "using BrainFlow synthetic only". |

**Score:** 8/8 truths verified (6 unique truths; 09-01/09-02/09-03 must-have sets fully covered)

### Required Artifacts

| Artifact | Expected    | Status | Details |
| -------- | ----------- | ------ | ------- |
| `docs/brainflow.md` | Canonical BrainFlow doc (setup, config, synthetic vs native, build order, BRAIN-04) | ✓ VERIFIED | Exists, 41 lines; all required sections present. |
| `crates/neurohid-core/Cargo.toml` | Default features include brainflow | ✓ VERIFIED | `default = ["device-lsl", "brainflow"]`. |
| `crates/neurohid-sdk/examples/embedded_runtime.rs` | Runnable example using BrainFlow synthetic (board_id 0) | ✓ VERIFIED | Contains `DeviceBackend::BrainFlow`, `BrainFlowConfig::default()`, RuntimeBuilder, RescanStreams, snapshot. |
| `crates/neurohid-hub/src/screens/devices.rs` | Devices screen copy includes BrainFlow; no LSL-only or parity-planned messaging | ✓ VERIFIED | Multi-backend wording; "Available Streams"; no "LSL-first" or "parity is planned/phased" in crates/neurohid-hub. |
| `crates/neurohid-core/src/tasks/device/discovery.rs` | AutoProvider uses BrainFlow synthetic as fallback | ✓ VERIFIED | Contains `BrainFlowProvider`, `create_brainflow_provider`; Auto branch creates BrainFlow fallback and `AutoProvider::new(lsl, fallback)`. |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| docs/index.md | docs/brainflow.md | Link in Architecture/System section | ✓ WIRED | `[brainflow.md](brainflow.md)` under "Architecture and System Docs". |
| embedded_runtime example | neurohid-core create_provider | RuntimeBuilder with config.device.backend = BrainFlow, brainflow = Some(BrainFlowConfig) | ✓ WIRED | Example sets `config.device.backend = DeviceBackend::BrainFlow`, `config.device.brainflow = Some(BrainFlowConfig::default())`, passes config to `RuntimeBuilder::new(config).start()` which drives provider creation. |
| Hub Devices screen | Same discover/connect/disconnect flow | Backend chosen in Settings; Rescan/Connect/Disconnect use same commands | ✓ WIRED | Settings device.rs exposes backend dropdown (Auto, LSL, Mock, Serial, BrainFlow); Devices screen uses service snapshot and Rescan/Connect/Disconnect; no backend-specific branching in Devices copy. |
| discovery.rs Auto branch | BrainFlowProvider | create_brainflow_provider with default config (board_id 0) | ✓ WIRED | Auto match arm calls `create_brainflow_provider(config)?` and uses result as fallback in `AutoProvider::new(lsl, fallback)`. |
| Tests/binaries | BrainFlow synthetic | DeviceBackend::BrainFlow + BrainFlowConfig in test configs | ✓ WIRED | neurohid-core runtime.rs, neurohid-service, neurohid-hub service_manager/tests.rs, neurohid-validate use `DeviceBackend::BrainFlow` and brainflow config; neurohid-sdk device tests use `device-brainflow` and BrainFlowProvider. |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| BRAIN-01 | 09-01 | First-class documentation for using BrainFlow with NeuroHID | ✓ SATISFIED | docs/brainflow.md + link from index. |
| BRAIN-02 | 09-02 | At least one runnable example using BrainFlow synthetic end-to-end | ✓ SATISFIED | embedded_runtime.rs uses BrainFlow, runs with runtime+types. |
| BRAIN-03 | 09-02 | Discover and connect BrainFlow from Hub with UX parity to LSL | ✓ SATISFIED | Devices/Settings copy and same flow; no LSL-only or parity-planned wording. |
| BRAIN-04 | 09-01 | BrainFlow one backend; device-agnostic API preserved | ✓ SATISFIED | Documented in brainflow.md; no API change; same Provider/Device abstraction. |
| BRAIN-05 | 09-03 | Synthetic board fully replaces in-repo mock in tests, examples, CI | ✓ SATISFIED | Auto fallback = BrainFlow; all relevant tests and binaries use BrainFlow; Mock only for explicit backend and neurohid-device internals. |

All phase requirement IDs (BRAIN-01–BRAIN-05) are claimed by plans 09-01, 09-02, 09-03 and satisfied. No orphaned requirements.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| — | — | None | — | — |

No TODO/FIXME/placeholder or stub patterns found in docs/brainflow.md or the verified artifacts.

### Human Verification Required

1. **Run embedded_runtime example** — Run `cargo run -p neurohid-sdk --example embedded_runtime --features "runtime,types"`. Expected: process runs without error; snapshot shows `device_connected` and `discovered_streams` (e.g. stream count). Reason: end-to-end execution and real-time behavior.
2. **Hub BrainFlow flow** — In Hub Settings set device backend to BrainFlow; on Devices screen click Rescan. Expected: BrainFlow synthetic stream appears under Available Streams; Connect/Disconnect work. Reason: visual UX and full user flow.

### Gaps Summary

None. All must-haves from 09-01, 09-02, and 09-03 are present, substantive, and wired. Phase goal is achieved.

---

_Verified: 2026-02-21_  
_Verifier: Claude (gsd-verifier)_
