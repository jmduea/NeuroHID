---
phase: 01-contracts-and-versioned-formats
plan: 02
subsystem: formats
tags: profile, serde, versioning, reproducibility, calibration

# Dependency graph
requires: []
provides:
  - ProfileMetadata with format_version and calibration_identity; profile-format.md with version, compatibility policy, and reproducibility identity
affects: profile load/save, export/import, calibration persistence

# Tech tracking
tech-stack:
  added: []
  patterns: format_version + optional identity in metadata for roundtrip; compatibility policy in same doc as format spec

key-files:
  created: docs/formats/profile-format.md
  modified: crates/neurohid-types/src/profile.rs, crates/neurohid-storage/src/profile.rs

key-decisions:
  - "Calibration identity stored in profile metadata (not separate manifest) so export/import roundtrip without extra files"
  - "Readers support N=2 previous format versions; additive changes use serde(default)"

patterns-established:
  - "Profile format version and compatibility policy live in one doc (profile-format.md)"
  - "Reproducibility identity (version + optional content_hash) in ProfileMetadata for re-run and audit/share"

requirements-completed: [COMP-05, PATH-03]

# Metrics
duration: 0
completed: 2026-02-20
---

# Phase 01 Plan 02: Profile format version and reproducibility Summary

**Profile metadata versioned with format_version and calibration identity; profile-format.md documents version, N=2 compatibility policy, and reproducibility (re-run and audit/share).**

## Performance

- **Duration:** (execution time)
- **Started:** (ISO timestamp)
- **Completed:** 2026-02-20
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- ProfileMetadata has `format_version` (u32, default 1) and `calibration_identity` (Option<CalibrationIdentity>) with serde defaults for backward compatibility
- CalibrationIdentity struct (format_version, optional content_hash) written when save_calibration is called; roundtrips on export_profile/import_profile
- docs/formats/profile-format.md: format version, compatibility policy (N=2), BNF schema, reproducibility identity (where stored, same-setup definition, re-run and audit/share)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add format_version and calibration identity to profile types and storage** - `ca0b337` (feat)
2. **Task 2: Document profile format, compatibility, and reproducibility identity** - `f1063dc` (docs)

## Files Created/Modified

- `docs/formats/profile-format.md` - Profile format version, compatibility policy, schema (BNF), reproducibility identity
- `crates/neurohid-types/src/profile.rs` - ProfileMetadata.format_version, CalibrationIdentity, calibration_identity; ProfileMetadata::new sets defaults
- `crates/neurohid-storage/src/profile.rs` - save_calibration updates metadata with calibration identity; roundtrip test

## Decisions Made

- Calibration identity stored in profile metadata only (no calibration_manifest.json) so export/import preserve it without extra files
- N=2 for reader compatibility (support current and previous two versions)
- content_hash in CalibrationIdentity left optional; storage sets format_version only (no new hash dependency)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Profile format is versioned and documented; COMP-05 and PATH-03 satisfied for profile
- Calibration and profile metadata stored with version/identity; doc explains re-run and audit/share

## Self-Check: PASSED

- FOUND: docs/formats/profile-format.md
- FOUND: .planning/phases/01-contracts-and-versioned-formats/01-02-SUMMARY.md
- FOUND: commits ca0b337, f1063dc

---
*Phase: 01-contracts-and-versioned-formats*
*Completed: 2026-02-20*
