---
phase: 01-contracts-and-versioned-formats
verified: "2026-02-20T00:00:00Z"
status: passed
score: 3/3 must-haves verified
---

# Phase 01: Contracts and Versioned Formats — Verification Report

**Phase Goal:** Config, profile, and stream semantics are versioned and documented so the same setup can be reproduced.

**Verified:** 2026-02-20  
**Status:** passed  
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths (Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Profile and config formats have a documented version and compatibility policy | ✓ VERIFIED | `docs/formats/config-format.md`: format version 1, N=2 compatibility, schema. `docs/formats/profile-format.md`: format version 1, N=2 compatibility, schema. Both in one doc per format. |
| 2 | Stream consumption, timestamp, and "latest sample" semantics are documented (e.g. drain-then-last for LSL) | ✓ VERIFIED | `docs/formats/stream-semantics.md`: consumption model, timestamps (remote capture → μs), latest-sample (drain-then-last and continuous pull with 0.2s), overflow; LSL full, Serial/BrainFlow/Mock brief. LSL device `crates/neurohid-device/src/lsl/device.rs` module doc references spec and states pull_sample(0.2), continuous forward. |
| 3 | Calibration and profile metadata are stored with version/identity so the same setup can be reproduced | ✓ VERIFIED | `ProfileMetadata` has `format_version` and `calibration_identity: Option<CalibrationIdentity>` (format_version + content_hash). `save_calibration` sets `calibration_identity` and calls `save_metadata`; `profile_metadata_format_version_and_calibration_identity_roundtrip` tests export/import roundtrip. `docs/formats/profile-format.md` documents where identity is stored, re-run and audit/share use cases. |

**Score:** 3/3 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `docs/formats/config-format.md` | Config format version, compatibility policy, schema (min 40 lines) | ✓ VERIFIED | 56 lines; format_version, N=2, BNF schema, "Where the version appears". |
| `docs/formats/profile-format.md` | Profile format version, compatibility, reproducibility identity (min 50 lines) | ✓ VERIFIED | 54 lines; format_version, N=2, schema with CalibrationIdentity, "Reproducibility identity" (where stored, re-run, audit/share). |
| `docs/formats/stream-semantics.md` | Stream semantics: consumption, timestamps, latest-sample, overflow (min 80 lines) | ✓ VERIFIED | 98 lines; LSL full (consumption, timestamps, latest-sample, overflow), Serial/BrainFlow/Mock brief; COMP-04 relation. |
| `crates/neurohid-types/src/config.rs` | SystemConfig with format_version | ✓ VERIFIED | `format_version: u32` with `#[serde(default)]`, default 1. |
| `crates/neurohid-storage/src/config.rs` | Config load/save with version handling | ✓ VERIFIED | load/save `SystemConfig`; tests roundtrip format_version and legacy TOML without field → 1. |
| `crates/neurohid-types/src/profile.rs` | ProfileMetadata with format_version and calibration identity | ✓ VERIFIED | `format_version`, `calibration_identity: Option<CalibrationIdentity>` (format_version, content_hash). |
| `crates/neurohid-storage/src/profile.rs` | Profile load/save and calibration identity handling | ✓ VERIFIED | `save_calibration` sets `metadata.calibration_identity` and `save_metadata`; roundtrip test. |
| `crates/neurohid-device/src/lsl/device.rs` | LSL device; reference to stream semantics doc | ✓ VERIFIED | Module doc links to `docs/formats/stream-semantics.md`, documents pull_sample(0.2), continuous forward. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| crates/neurohid-storage/src/config.rs | crates/neurohid-types/src/config.rs | serialize/deserialize SystemConfig with format_version | ✓ WIRED | Uses `SystemConfig`; load/save and tests assert format_version. |
| docs/formats/config-format.md | COMP-05 | same doc contains version and compatibility policy | ✓ WIRED | Doc contains format_version, compatibility (N=2), schema. |
| crates/neurohid-storage/src/profile.rs | crates/neurohid-types/src/profile.rs | serialize/deserialize ProfileMetadata with format_version | ✓ WIRED | get_metadata/save_metadata/save_calibration use ProfileMetadata, CalibrationIdentity. |
| docs/formats/profile-format.md | PATH-03 | doc states where identity is stored and re-run vs audit/share | ✓ WIRED | "Reproducibility identity" section: where stored, re-run, audit/share. |
| docs/formats/stream-semantics.md | COMP-04 | doc defines consumption, timestamps, latest-sample per stream type | ✓ WIRED | Full LSL + brief others; consumption, timestamp, latest sample, drain. |
| crates/neurohid-device/src/lsl/device.rs | docs/formats/stream-semantics.md | comment or doc link so implementation aligns with spec | ✓ WIRED | Module doc: "Consumption model, timestamps, and 'latest sample' semantics are defined in docs/formats/stream-semantics.md". |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| PATH-03 | 01-02 | Calibration and profile metadata stored with version/identity so the same setup can be reproduced | ✓ SATISFIED | ProfileMetadata.format_version + calibration_identity; profile-format.md (re-run, audit/share); save_calibration persists identity; roundtrip test. |
| COMP-04 | 01-03 | Stream consumption, timestamp, and "latest sample" semantics documented and consistent (e.g. drain-then-last for LSL) | ✓ SATISFIED | stream-semantics.md; LSL device references doc; drain-then-last and continuous 0.2s documented. |
| COMP-05 | 01-01, 01-02 | Profile and config formats versioned and have a documented compatibility policy | ✓ SATISFIED | config-format.md and profile-format.md each have version + N=2 compatibility in same doc; types have format_version. |

No requirement IDs for Phase 01 are orphaned (all PATH-03, COMP-04, COMP-05 claimed by 01-01, 01-02, 01-03).

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (none) | — | — | — | No TODO/FIXME/placeholder in docs/formats or verified crate files. |

### Human Verification Required

None. All must-haves are verifiable from docs and code; no visual or runtime-only behavior required for this phase.

### Gaps Summary

None. All three success criteria are met: config and profile formats are versioned and documented with compatibility policy; stream semantics (consumption, timestamps, latest-sample including drain-then-last for LSL) are documented; calibration and profile metadata are stored with version/identity and roundtrip on export/import, with documentation for re-run and audit/share.

---

_Verified: 2026-02-20_  
_Verifier: Claude (gsd-verifier)_
