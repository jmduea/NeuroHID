---
phase: 06-composable-and-extensible
plan: 02
subsystem: runtime
tags: [extensions, libloading, config, device, outlet, signal, decoder, snapshot]

# Dependency graph
requires:
  - phase: 06-01
    provides: ExtensionRegistry, outlet/signal/decoder contracts, ExtensionManifest
provides:
  - Config extension selection for all four slots (device, signal, decoder, outlet)
  - Device/outlet/signal/decoder factories with built-in or registry-loaded extensions
  - libloading-based dylib loading; clear errors on unknown name or load failure
  - ControlSnapshot and ServiceState slot names (device, outlet, signal, decoder)
affects: [06-03, 06-04, hub, sdk]

# Tech tracking
tech-stack:
  added: [libloading 0.8]
  patterns: [trait-based slot + registry load, Loaded* wrapper holding Library guard]

key-files:
  created: []
  modified:
    - crates/neurohid-types/src/config.rs
    - crates/neurohid-types/src/error.rs
    - crates/neurohid-types/src/outlet.rs
    - crates/neurohid-types/src/signal_contract.rs
    - crates/neurohid-types/src/decoder_contract.rs
    - crates/neurohid-types/src/control.rs
    - crates/neurohid-core/src/extension_registry.rs
    - crates/neurohid-core/src/tasks/device.rs
    - crates/neurohid-core/src/tasks/outlet.rs
    - crates/neurohid-core/src/tasks/signal.rs
    - crates/neurohid-core/src/tasks/decoder.rs
    - crates/neurohid-core/src/service.rs
    - crates/neurohid-core/src/runtime.rs

key-decisions:
  - "Device backend Extension(name) as enum variant; outlet/signal/decoder use optional extension_name in config"
  - "Registry stores full ExtensionManifest (with optional library path) for load_* methods"
  - "Snapshot exposes all four slot names (device_name, outlet_name, signal_name, decoder_name) for Hub/CLI"

patterns-established:
  - "Loaded* wrapper: holds libloading::Library + Box<dyn Trait>; implements trait by delegation; library not unloaded while in use"
  - "create_* factory: returns (Box<dyn Trait>, display_name); built-in path uses existing task; extension path calls registry.load_* and returns Loaded*"

requirements-completed: [COMP-06, EXT-01, EXT-02]

# Metrics
duration: ~45min
completed: 2026-02-21
---

# Phase 6 Plan 2: Name-based selection and factories Summary

**Config selects all four pipeline slots (device, signal, decoder, outlet) by built-in or extension name; device/outlet/signal/decoder factories load extensions via registry and libloading; snapshot reflects slot names; load failure is explicit with no silent fallback.**

## Performance

- **Duration:** ~45 min
- **Tasks:** 3
- **Files modified:** 20+

## Accomplishments

- Device: `DeviceBackend::Extension(name)`, `create_provider` resolves via registry and libloading; `LoadedDeviceProvider` holds library guard; clear error on unknown name or load failure.
- Outlet: `OutletConfig.extension_name`, `create_outlet()` returns built-in `OutletTask` or `LoadedOutlet`; service spawns `Box<dyn Outlet>`; `outlet_name` in state and snapshot.
- Signal: `SignalConfig.extension_name`, `SignalChannels`, `create_signal_preprocessor()`, `load_signal_preprocessor` in registry; `SignalTask` implements `SignalPreprocessor`; `signal_name` in snapshot.
- Decoder: `DecoderConfig.extension_name`, `DecoderChannels`, `create_decoder()`, `load_decoder` in registry; `DecoderTask` implements `DecoderRunner`; `decoder_name` in snapshot.
- ControlSnapshot and ServiceState extended with `outlet_name`, `signal_name`, `decoder_name`; runtime snapshot and Hub/test literals updated.

## Task Commits

1. **Task 1: Device config extension selection and create_provider extension path** - `d71afe1` (feat)
2. **Task 2: Outlet config extension selection and outlet factory** - `b5d161b` (feat)
3. **Task 3: Signal preprocessing and decoder config and factories** - `292a3ee` (feat)

## Files Created/Modified

- `crates/neurohid-types/src/config.rs` - DeviceBackend::Extension, OutletConfig/SignalConfig/DecoderConfig.extension_name
- `crates/neurohid-types/src/error.rs` - ExtensionError::NotFound, LoadError
- `crates/neurohid-types/src/outlet.rs` - ExtensionManifest.library; SignalChannels/DecoderChannels in contract modules
- `crates/neurohid-core/src/extension_registry.rs` - load_device_provider, load_outlet, load_signal_preprocessor, load_decoder; Loaded* wrappers
- `crates/neurohid-core/src/tasks/device.rs` - registry param, Extension(name) branch in create_provider
- `crates/neurohid-core/src/tasks/outlet.rs` - Outlet impl for OutletTask, create_outlet
- `crates/neurohid-core/src/tasks/signal.rs` - SignalPreprocessor impl, create_signal_preprocessor
- `crates/neurohid-core/src/tasks/decoder.rs` - DecoderRunner impl, create_decoder
- `crates/neurohid-core/src/service.rs` - registry before outlet; create_outlet/create_signal_preprocessor/create_decoder; slot names in state
- `crates/neurohid-core/src/runtime.rs` - outlet_name, signal_name, decoder_name in snapshot
- `crates/neurohid-types/src/control.rs` - outlet_name, signal_name, decoder_name in ControlSnapshot

## Decisions Made

- Extension identity remains name-only (no version in ID) per CONTEXT.
- In-process plugins must be built with same Rust toolchain (documented; Loaded* wrappers keep library alive).
- Hub settings: DeviceBackend::Extension handled in backend label and selector (Extension option with placeholder name when selected).

## Deviations from Plan

None - plan executed as written. Minor additions: Hub settings match on DeviceBackend::Extension; ControlSnapshot test literals and ipc/hub/neurohid-service test snapshots updated for new fields.

## Issues Encountered

- device_registry was moved into device spawn closure; fixed by cloning before spawn and using clone for device task.
- Several ControlSnapshot struct literals (control, ipc, hub, neurohid-service) needed outlet_name, signal_name, decoder_name, and in one case recording_active/current_session_id.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- All four slots support config-based extension selection and factory loading.
- 06-03 (example outlet plugin and CI) can build an outlet extension that exports `neurohid_outlet_create` and is loaded by name.
- Snapshot and observability show slot names for Hub/CLI.

---
*Phase: 06-composable-and-extensible*
*Completed: 2026-02-21*
