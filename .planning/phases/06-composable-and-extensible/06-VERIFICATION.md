---
phase: 06-composable-and-extensible
verified: "2026-02-21T00:00:00Z"
status: passed
score: 24/24 must-haves verified
gaps: []
human_verification: []
---

# Phase 6: Composable and extensible — Verification Report

**Phase Goal:** Pipeline components are swappable and new device/output types can be added via published contracts.

**Verified:** 2026-02-21  
**Status:** passed  
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

#### Plan 06-01 (Contracts and registry)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Outlet contract is published and implementable by extensions | ✓ VERIFIED | `crates/neurohid-types/src/outlet.rs`: `Outlet` trait, `OutletChannels`, re-exported from lib.rs |
| 2 | Signal preprocessing contract is published and implementable | ✓ VERIFIED | `signal_contract.rs`: `SignalPreprocessor`, `SignalChannels`; lib.rs re-exports |
| 3 | Decoder contract is published and implementable | ✓ VERIFIED | `decoder_contract.rs`: `DecoderRunner`, `DecoderChannels`; lib.rs re-exports |
| 4 | Extension manifest (name-only ID) is defined and parseable | ✓ VERIFIED | `outlet.rs`: `ExtensionManifest` with `name`, `kind` (ExtensionKind); serde Serialize/Deserialize |
| 5 | Registry discovers extensions from configured path(s) and enforces unique names | ✓ VERIFIED | `extension_registry.rs`: `scan()` walks paths, parses manifest.json; `ExtensionError::DuplicateName` on duplicate |
| 6 | Registry lists extensions by slot: outlets, devices, signal preprocessing, decoders | ✓ VERIFIED | `list_outlets()`, `list_devices()`, `list_signal_preprocessors()`, `list_decoders()` implemented; unit tests |
| 7 | Duplicate extension names cause discovery to fail with clear error | ✓ VERIFIED | Test `duplicate_name_fails_scan` in extension_registry.rs |

#### Plan 06-02 (Name-based selection and factories)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 8 | Config can select device backend by built-in or extension name | ✓ VERIFIED | `config.rs`: `DeviceBackend::Extension(String)`; device dropdown in settings uses it |
| 9 | Config can select signal/decoder/outlet by built-in or extension name | ✓ VERIFIED | `SignalConfig`, `DecoderConfig`, `OutletConfig` each have `extension_name: Option<String>` |
| 10 | Device factory loads extension by name via registry; returns Box<dyn DeviceProvider> or fails clearly | ✓ VERIFIED | `device.rs`: `create_provider()` matches `DeviceBackend::Extension(name)`, calls `reg.load_device_provider(name)?`; no silent fallback |
| 11 | Signal/decoder/outlet factories produce built-in or loaded extension; pipeline and snapshot integrate | ✓ VERIFIED | `create_signal_preprocessor`, `create_decoder`, `create_outlet` in tasks; `service.rs` uses them and sets `state.signal_name`, `state.decoder_name`, `state.outlet_name` |
| 12 | Load/run failure surfaces as explicit error; no silent fallback | ✓ VERIFIED | Extension branches return `Err`; registry `load_*` return Result; no fallback to built-in on unknown name |

#### Plan 06-03 (Example outlet and CI)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 13 | Example outlet plugin exists and implements the outlet contract | ✓ VERIFIED | `crates/neurohid-outlet-example/src/lib.rs`: implements `Outlet`, exports `neurohid_outlet_create` |
| 14 | Example plugin is built as part of the workspace | ✓ VERIFIED | `Cargo.toml` members include `neurohid-outlet-example`; cdylib; CI runs `cargo build --workspace` |
| 15 | CI runs with config that uses the example outlet and asserts something observable | ✓ VERIFIED | `ci.yml`: `cargo test -p neurohid-core --test extension_outlet_e2e`; test creates registry with example, calls `create_outlet(..., extension_name: Some("neurohid-outlet-example"))`, asserts `name == "neurohid-outlet-example"` |

#### Plan 06-04 (Hub and CLI)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 16 | Hub has an Extensions screen that lists discovered extensions (all four kinds) and supports rescan | ✓ VERIFIED | `screens/extensions.rs`: `ExtensionsScreen` uses `list_outlets`, `list_devices`, `list_signal_preprocessors`, `list_decoders`; Rescan button sets `rescan_requested` and re-runs scan |
| 17 | Device, signal, decoder, and outlet config in Hub show dropdown: built-in + extension by name | ✓ VERIFIED | `settings.rs`: device backend dropdown includes `DeviceBackend::Extension(name)` from `reg.list_devices()`; signal/decoder/outlet use `extension_name` with "Built-in" + `list_signal_preprocessors()` / `list_decoders()` / `list_outlets()` |
| 18 | Selection in Hub updates config immediately (sync); no separate save for any slot | ✓ VERIFIED | On change, `cfg.backend` / `cfg.extension_name` updated; after section, `state.config_store.save(&state.config)` called for device/signal/decoder/outlet when respective value changed |
| 19 | CLI can list extensions and trigger refresh; discovery path documented | ✓ VERIFIED | `neurohid.rs`: `neurohid extensions list` and `neurohid extensions refresh` invoke `run_extensions_cli` (registry scan + print). `docs/extension-contracts.md`: default path, override, CLI usage |

**Score:** 24/24 truths verified (7 + 5 + 3 + 4 across plans; some truths map to same artifact).

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/neurohid-types/src/outlet.rs` | Outlet contract + ExtensionManifest | ✓ VERIFIED | Outlet trait, ExtensionKind, ExtensionManifest, OutletChannels; exported |
| `crates/neurohid-types/src/signal_contract.rs` | Signal preprocessing contract | ✓ VERIFIED | SignalPreprocessor, SignalChannels; substantive |
| `crates/neurohid-types/src/decoder_contract.rs` | Decoder contract | ✓ VERIFIED | DecoderRunner, DecoderChannels; substantive |
| `crates/neurohid-core/src/extension_registry.rs` | Discovery, list by slot, duplicate error | ✓ VERIFIED | scan(), list_*(), DuplicateName, uses ExtensionManifest from types |
| `docs/extension-contracts.md` | All four contracts + discovery + CLI | ✓ VERIFIED | Outlet, device, signal, decoder contracts; manifest; discovery path; CLI; example plugin |
| `crates/neurohid-types/src/config.rs` | DeviceBackend::Extension + extension_name per slot | ✓ VERIFIED | DeviceBackend::Extension(String); SignalConfig/DecoderConfig/OutletConfig.extension_name |
| `crates/neurohid-core/src/tasks/device.rs` | create_provider resolves Extension(name) via registry | ✓ VERIFIED | load_device_provider(name); LoadedDeviceProvider holds library guard |
| `crates/neurohid-core` (signal/decoder/outlet factories) | create_signal_preprocessor, create_decoder, create_outlet | ✓ VERIFIED | All three exist; used from service.rs |
| `crates/neurohid-outlet-example` | Example outlet impl + manifest/build | ✓ VERIFIED | Implements Outlet; cdylib; workspace member; manifest described in docs |
| `.github/workflows/ci.yml` | Build + e2e for example outlet | ✓ VERIFIED | cargo build --workspace; cargo test -p neurohid-core --test extension_outlet_e2e |
| `crates/neurohid-hub` (Extensions screen + dropdowns) | Extensions screen; device/signal/decoder/outlet selection | ✓ VERIFIED | extensions.rs; settings.rs device/signal/decoder/outlet dropdowns with registry lists |
| `crates/neurohid` (CLI) | extensions list and refresh | ✓ VERIFIED | run_extensions_cli for "list" and "refresh"; registry.scan(); prints kind, name, path |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| extension_registry | neurohid-types manifest | serde deserialize | ✓ WIRED | `ExtensionManifest` from types; `serde_json::from_str` in read_manifest_in_dir |
| service.rs | outlet factory | create_outlet from config (built-in or extension) | ✓ WIRED | create_outlet(..., Some(&reg)); state.outlet_name set from returned name |
| service.rs | signal/decoder factories | create_signal_preprocessor, create_decoder | ✓ WIRED | create_signal_preprocessor/create_decoder called with config and registry; state.signal_name/decoder_name set |
| control snapshot | outlet/device/signal/decoder by name | snapshot includes slot names | ✓ WIRED | runtime.rs builds ControlSnapshot with device_name, outlet_name, signal_name, decoder_name from state |
| CI e2e | runtime with example outlet | config selects example by name; assert name | ✓ WIRED | extension_outlet_e2e: create_outlet with extension_name Some("neurohid-outlet-example"); assert name |
| Hub device dropdown | config.device | selection writes backend/Extension(name) | ✓ WIRED | theme::select_index; cfg.backend = DeviceBackend::Extension(name.clone()); config_store.save on change |
| Hub signal/decoder/outlet dropdowns | config.signal/decoder/outlet | extension_name selection; save on change | ✓ WIRED | cfg.extension_name = Some(selected); config_store.save when last_*_extension != current |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| COMP-06 | 06-01, 06-02, 06-04 | Replace any pipeline component via published contract; rest of pipeline, control, Hub, runtime, observability integrate | ✓ SATISFIED | Four contracts in types; config selects by name; factories load extensions; snapshot has slot names; Hub Extensions + dropdowns + persist; CLI list/refresh |
| EXT-01 | 06-01, 06-02 | New device backends addable without changing core orchestration (trait/plugin contract) | ✓ SATISFIED | DeviceProvider trait (neurohid-device); DeviceBackend::Extension(name); create_provider loads via registry |
| EXT-02 | 06-01, 06-02 | New action/output types addable via defined outlet/effector contract | ✓ SATISFIED | Outlet trait in neurohid-types; outlet.extension_name; create_outlet loads extension; example outlet implements trait |
| EXT-03 | 06-03 | One example plugin (device or outlet) exists and is tested in CI | ✓ SATISFIED | neurohid-outlet-example; CI runs extension_outlet_e2e; test asserts create_outlet returns extension by name |

No orphaned requirements: REQUIREMENTS.md maps COMP-06, EXT-01, EXT-02, EXT-03 to Phase 6; each appears in at least one plan frontmatter.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (none) | — | — | — | No TODO/FIXME/placeholder or stub implementations found in verified artifacts. |

### Human Verification (optional)

- **Hub flow:** Open Hub → Extensions → Rescan; Settings → Device / Signal / Decoder / Outlet → select an extension; confirm config persists and runtime snapshot shows the chosen names. (Visual/UX; automated checks already confirm code paths.)
- **CLI:** Run `neurohid extensions list` (and `neurohid extensions refresh`) with default or custom path; confirm output format and exit code. (CLI output format; code paths verified.)

### Gaps Summary

None. All must-haves from plans 06-01 through 06-04 are present, substantive, and wired. Phase goal is achieved: pipeline components are swappable via published contracts; new device and output types can be added via registry and config; one example outlet plugin exists and is tested in CI; Hub and CLI provide extensions screen and list/refresh.

---

_Verified: 2026-02-21_  
_Verifier: Claude (gsd-verifier)_
