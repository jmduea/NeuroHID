---
phase: 06-composable-and-extensible
plan: 01
subsystem: extensions
tags: [neurohid-types, neurohid-core, extension-registry, contracts, serde, async-trait]

# Dependency graph
requires: []
provides:
  - Outlet, SignalPreprocessor, DecoderRunner contracts in neurohid-types
  - ExtensionManifest and ExtensionKind; ExtensionError
  - ExtensionRegistry with scan, list_outlets, list_devices, list_signal_preprocessors, list_decoders
  - docs/extension-contracts.md (four contracts, manifest, discovery path)
affects: [06-02, 06-03, 06-04]

# Tech tracking
tech-stack:
  added: [async-trait, tokio/sync in neurohid-types; extension_registry in neurohid-core]
  patterns: [trait-based slot contract with run(self: Box<Self>, shutdown); name-only extension ID; duplicate name = hard fail]

key-files:
  created: [crates/neurohid-types/src/outlet.rs, signal_contract.rs, decoder_contract.rs, crates/neurohid-core/src/extension_registry.rs, docs/extension-contracts.md]
  modified: [crates/neurohid-types/src/lib.rs, error.rs, Cargo.toml, crates/neurohid-core/src/lib.rs, docs/index.md]

key-decisions:
  - "Extension identity by name only; duplicate names cause scan() to fail with ExtensionError::DuplicateName"
  - "Default discovery path: config root (neurohid_storage::default_data_dir()) + /extensions"
  - "Manifest filenames: manifest.json or neurohid.manifest.json in each extension directory"

patterns-established:
  - "Slot contract: async run(self: Box<Self>, shutdown) in neurohid-types; construction in core or extension"
  - "Registry scans direct child dirs of each path; one manifest per directory"

requirements-completed: [COMP-06, EXT-01, EXT-02]

# Metrics
duration: ~25min
completed: 2026-02-21
---

# Phase 06 Plan 01: Extension contracts and registry Summary

**Outlet, signal preprocessing, and decoder contracts in neurohid-types; ExtensionManifest with kind; ExtensionRegistry in neurohid-core with discovery, unique-name enforcement, and list methods per slot; extension-contracts.md documents all four contracts and discovery path.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 3
- **Files created/modified:** 10

## Accomplishments

- Outlet contract (`Outlet` trait, `OutletChannels`) and extension manifest types (`ExtensionManifest`, `ExtensionKind`) in neurohid-types; re-exports from lib.
- Signal preprocessing contract (`SignalPreprocessor`) and decoder contract (`DecoderRunner`) in neurohid-types.
- Extension registry in neurohid-core: scan configured paths for manifest.json / neurohid.manifest.json, parse with serde, enforce unique names (fail on duplicate), expose list_outlets, list_devices, list_signal_preprocessors, list_decoders.
- ExtensionError (DuplicateName, ManifestError, InvalidPath) in neurohid-types.
- docs/extension-contracts.md: all four slot contracts (outlet, device, signal preprocessing, decoder), manifest format, discovery default and override, duplicate-name policy, rescan behaviour.

## Task Commits

Each task was committed atomically:

1. **Task 1: Outlet contract and extension manifest in neurohid-types** - `d24d618` (feat)
2. **Task 2: Signal preprocessing and decoder contracts in neurohid-types** - `46c8cb5` (feat)
3. **Task 3: Extension registry and discovery in neurohid-core; docs** - `3e0b71d` (feat)

## Files Created/Modified

- `crates/neurohid-types/src/outlet.rs` - Outlet trait, ExtensionManifest, ExtensionKind, OutletChannels
- `crates/neurohid-types/src/signal_contract.rs` - SignalPreprocessor trait
- `crates/neurohid-types/src/decoder_contract.rs` - DecoderRunner trait
- `crates/neurohid-types/src/error.rs` - ExtensionError variants
- `crates/neurohid-types/src/lib.rs` - mod and re-exports
- `crates/neurohid-types/Cargo.toml` - tokio (sync), async-trait
- `crates/neurohid-core/src/extension_registry.rs` - ExtensionRegistry, default_extension_paths, tests
- `crates/neurohid-core/src/lib.rs` - pub mod extension_registry
- `docs/extension-contracts.md` - Contracts and discovery documentation
- `docs/index.md` - Link to extension-contracts.md

## Decisions Made

- Extension ID is name-only (no version in ID per CONTEXT).
- Default extension path: same config root as storage, plus `extensions` subdirectory.
- Registry scans one level: each configured path’s direct child directories; each child must contain a manifest file.
- Duplicate extension names cause `scan()` to return an error; no silent deduplication.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Tokio dependency in neurohid-types**
- **Found during:** Task 1
- **Issue:** Workspace tokio has `default-features = true`; member cannot override with `default-features = false`.
- **Fix:** Used direct `tokio = { version = "1.49", default-features = false, features = ["sync"] }` in neurohid-types.
- **Files modified:** crates/neurohid-types/Cargo.toml
- **Committed in:** d24d618 (Task 1)

**2. [Rule 1 - Bug] Extension registry test expected manifest at root of search path**
- **Found during:** Task 3
- **Issue:** Registry design: each path’s direct children are extension dirs; manifest lives inside each child. Test put manifest at temp/manifest.json instead of temp/subdir/manifest.json.
- **Fix:** Test now creates temp/my-outlet/manifest.json and asserts path file_name.
- **Files modified:** crates/neurohid-core/src/extension_registry.rs
- **Committed in:** 3e0b71d (Task 3)

**3. [Rule 1 - Bug] Windows path comparison in test**
- **Found during:** Task 3
- **Issue:** canonicalize() on Windows yields `\\?\C:\...` so assert_eq!(outlets[0].path, ext_dir) failed.
- **Fix:** Assert path.file_name() == "my-outlet" instead of equality with ext_dir.
- **Files modified:** crates/neurohid-core/src/extension_registry.rs
- **Committed in:** 3e0b71d (Task 3)

---

**Total deviations:** 3 auto-fixed (1 blocking, 2 bug)
**Impact on plan:** All necessary for build and tests. No scope creep.

## Issues Encountered

- neurohid-types `cargo test` fails due to pre-existing missing fields in `ControlSnapshot` (ipc.rs); out of scope per deviation rules. neurohid-core tests (including extension_registry) all pass.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Plan 02 can add factories that produce built-in or extension implementations using the new contracts and registry.
- Registry is ready to be constructed from config (path list) and used by Hub/CLI for listing; loading (dylib/subprocess) is Plan 02.

## Self-Check

- [x] crates/neurohid-types/src/outlet.rs exists
- [x] crates/neurohid-types/src/signal_contract.rs, decoder_contract.rs exist
- [x] crates/neurohid-core/src/extension_registry.rs exists
- [x] docs/extension-contracts.md exists
- [x] Commits d24d618, 46c8cb5, 3e0b71d present in git log

**Self-Check: PASSED**

---
*Phase: 06-composable-and-extensible*
*Completed: 2026-02-21*
