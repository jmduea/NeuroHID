---
phase: 01-contracts-and-versioned-formats
plan: 01
subsystem: config
tags: config, toml, format_version, serde, compatibility

# Dependency graph
requires: []
provides:
  - SystemConfig with format_version; config load/save roundtrip with version
  - docs/formats/config-format.md with version, N=2 compatibility policy, and schema/BNF
affects: []

# Tech tracking
tech-stack:
  added: []
  patterns: "format_version on root config struct with serde(default); single doc for version + compatibility + schema"

key-files:
  created: docs/formats/config-format.md
  modified: crates/neurohid-types/src/config.rs, crates/neurohid-storage/src/config.rs

key-decisions:
  - "Config format version and compatibility policy live in same doc as schema (config-format.md)"
  - "Readers support N = 2 previous format versions; breaking changes documented when external dependents exist"

patterns-established:
  - "Root config struct carries format_version with serde(default); legacy files load as version 1"

requirements-completed: [COMP-05]

# Metrics
duration: 15min
completed: "2026-02-20"
---

# Phase 01 Plan 01: Config format version and compatibility Summary

**SystemConfig has format_version (u32) with serde default 1; config-format.md documents version, N=2 compatibility policy, and TOML schema in one place.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 2
- **Files modified:** 3 (2 modified, 1 created)

## Accomplishments

- Added `format_version: u32` to `SystemConfig` with `#[serde(default)]` and `CURRENT_CONFIG_FORMAT_VERSION = 1`; explicit `Default` impl.
- Config load/save roundtrip persists `format_version`; legacy TOML without the key deserializes as version 1.
- Created `docs/formats/config-format.md` with format version, compatibility policy (N = 2), and BNF-style schema; satisfies COMP-05 for config.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add format_version to config types and storage** - `63b6bfb` (feat)
2. **Task 2: Document config format and compatibility policy** - `c003c57` (docs)

## Files Created/Modified

- `docs/formats/config-format.md` - Config format version, compatibility policy, and schema (created)
- `crates/neurohid-types/src/config.rs` - SystemConfig.format_version, constant, Default impl (modified)
- `crates/neurohid-storage/src/config.rs` - Tests for format_version roundtrip and legacy deserialize (modified)

## Decisions Made

- Version and compatibility policy in the same doc as the format spec (per user decision in CONTEXT).
- N = 2 for “readers support at least N previous format versions”.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Config format is versioned and documented; profile format (01-02) and stream semantics (01-03) can proceed or are already present on this branch.

## Self-Check: PASSED

- FOUND: docs/formats/config-format.md
- FOUND: .planning/phases/01-contracts-and-versioned-formats/01-01-SUMMARY.md
- FOUND: commits 63b6bfb, c003c57

---
*Phase: 01-contracts-and-versioned-formats*
*Completed: 2026-02-20*
