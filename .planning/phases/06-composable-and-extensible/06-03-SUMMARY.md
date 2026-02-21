---
phase: 06-composable-and-extensible
plan: 03
subsystem: extensions
tags: [outlet, cdylib, libloading, ci, e2e]

# Dependency graph
requires:
  - phase: 06-01
    provides: outlet contract (neurohid-types), ExtensionManifest
  - phase: 06-02
    provides: name-based selection, load_outlet, create_outlet with registry
provides:
  - One example outlet plugin (neurohid-outlet-example) implementing the outlet contract
  - CI build and e2e step that runs runtime path with example outlet and asserts observable creation
affects: [docs/extension-contracts, CI]

# Tech tracking
tech-stack:
  added: [neurohid-outlet-example crate (cdylib), integration test extension_outlet_e2e]
  patterns: [outlet extension factory symbol neurohid_outlet_create, manifest + dylib in extension dir]

key-files:
  created: [crates/neurohid-outlet-example/Cargo.toml, crates/neurohid-outlet-example/src/lib.rs, crates/neurohid-outlet-example/README.md, crates/neurohid-core/tests/extension_outlet_e2e.rs]
  modified: [Cargo.toml, Cargo.lock, docs/extension-contracts.md, crates/neurohid-core/Cargo.toml, .github/workflows/ci.yml]

key-decisions:
  - "Example outlet as workspace member (neurohid-outlet-example) for same toolchain and CI simplicity"
  - "E2E as in-process integration test: create_outlet with registry and example dir; assert outlet and name (no full pipeline run)"

patterns-established:
  - "Outlet extension: cdylib exporting neurohid_outlet_create(OutletConfig, OutletChannels) -> Result<Box<dyn Outlet>>; manifest with name, kind, optional library filename"

requirements-completed: [EXT-03]

# Metrics
duration: 6min
completed: 2026-02-21
---

# Phase 06 Plan 03: Example Outlet Plugin and CI E2E Summary

**One example outlet plugin (neurohid-outlet-example) as workspace cdylib implementing the outlet contract, with CI building the workspace and running an e2e test that loads the example and asserts outlet creation by name.**

## Performance

- **Duration:** 6 min
- **Started:** 2026-02-21T14:42:36Z
- **Completed:** 2026-02-21T14:48:57Z
- **Tasks:** 2 completed
- **Files modified:** 9 (6 created, 3 modified)

## Accomplishments

- Added `neurohid-outlet-example` crate: cdylib implementing `Outlet` (minimal run-until-shutdown), exporting `neurohid_outlet_create` per extension contract.
- Documented build and load in crate README and `docs/extension-contracts.md` (manifest, library filename, discovery).
- Added integration test `extension_outlet_e2e`: locates built example dylib, stages manifest + library in temp dir, scans registry, calls `create_outlet` with `extension_name = "neurohid-outlet-example"`, asserts `(outlet, name)` and name.
- CI: Test job (Linux/Windows matrix and macOS) now runs "Extension outlet e2e" step after `cargo test --workspace`.

## Task Commits

Each task was committed atomically:

1. **Task 1: Create neurohid-outlet-example crate and implement outlet contract** - `cb63b09` (feat)
2. **Task 2: CI build and e2e for example outlet** - `32eee1a` (feat)

## Files Created/Modified

- `crates/neurohid-outlet-example/Cargo.toml` - Package and cdylib config, deps (neurohid-types, tracing, tokio, async-trait)
- `crates/neurohid-outlet-example/src/lib.rs` - ExampleOutlet impl of Outlet, `neurohid_outlet_create` export
- `crates/neurohid-outlet-example/README.md` - Build and load instructions, manifest example
- `crates/neurohid-core/tests/extension_outlet_e2e.rs` - Integration test: stage extension dir, scan, create_outlet, assert name
- `Cargo.toml` - Added neurohid-outlet-example to workspace members
- `Cargo.lock` - Updated for new crate
- `docs/extension-contracts.md` - Loading section and "Example outlet plugin" subsection (build, manifest, discovery)
- `crates/neurohid-core/Cargo.toml` - dev-dependency neurohid-types for integration test
- `.github/workflows/ci.yml` - "Extension outlet e2e" step in Test and Test (macOS) jobs

## Decisions Made

- Example as workspace member (not separate repo/dir) for ABI alignment and simple CI.
- E2E implemented as in-process integration test asserting outlet creation and name rather than full pipeline/snapshot; sufficient for EXT-03 and avoids flaky process/snapshot wiring in CI.

## Deviations from Plan

None - plan executed as written. One small fix: `#[no_mangle]` required `#[unsafe(no_mangle)]` under edition 2024 (handled inline).

## Issues Encountered

None.

## Next Phase Readiness

- EXT-03 satisfied: one example plugin exists and is tested in CI.
- Plan 06-04 can proceed; example outlet is the reference for building and loading outlet extensions.

## Self-Check: PASSED

- crates/neurohid-outlet-example/src/lib.rs — FOUND
- crates/neurohid-core/tests/extension_outlet_e2e.rs — FOUND
- .planning/phases/06-composable-and-extensible/06-03-SUMMARY.md — FOUND
- Commits cb63b09, 32eee1a present in git log

---
*Phase: 06-composable-and-extensible*
*Completed: 2026-02-21*
