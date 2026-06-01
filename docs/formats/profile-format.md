# Profile format

Profile metadata and calibration identity are versioned so the same setup can be reproduced and compatibility is explicit. This document describes the profile metadata format, compatibility policy, and reproducibility identity in one place.

## Format version

- **Current profile metadata version:** `1`
- The root struct `ProfileMetadata` includes a `format_version` field (integer). Readers use it to select the correct deserializer or migration path.
- New profiles are written with `format_version: 1`. Older files that omit the field are treated as version 1 via `#[serde(default)]`.

## Compatibility policy

- **Readers** support at least **N = 2** previous format versions. For example, when current version is 3, readers must support versions 2 and 3.
- **Breaking changes** (e.g. removing or renaming required fields) require a new format version. Until external parties depend on this format, breaking changes are acceptable; when they do, document the break and migration steps in this doc or a changelog.
- **Additive changes** (new optional fields with defaults) do not require a new version; use `#[serde(default)]` so older files still deserialize.

## Schema (profile metadata)

Profile metadata is stored as JSON. Conceptual shape:

```bnf
ProfileMetadata     := format_version id name created_at last_used_at
                       last_calibrated_at? total_usage_time_us
                       calibration_state calibration_identity?

format_version      := integer   ; default 1
id                  := string   ; profile identifier
name                := string
created_at          := integer  ; microseconds since Unix epoch
last_used_at        := integer
last_calibrated_at  := integer | null
total_usage_time_us := integer
calibration_state   := "NotCalibrated" | "InProgress" | "CompletedPoor" | ...
calibration_identity := CalibrationIdentity | null

CalibrationIdentity := format_version content_hash?
format_version      := integer   ; calibration blob format, currently 1
content_hash        := string | null   ; optional e.g. SHA-256 hex for verification
```

The canonical source of the Rust types is `crates/neurohid-types/src/profile.rs` (`ProfileMetadata`, `CalibrationIdentity`, `CalibrationState`).

## Reproducibility identity

**Where identity is stored:** In profile metadata only. The `calibration_identity` field (optional) is written when calibration is saved; it is persisted in the same `metadata.json` as the rest of the profile. There is no separate manifest file next to `calibration.enc`; keeping identity in profile metadata ensures **export/import roundtrips** it without extra files.

**What identifies “same setup”:**

- Profile: `format_version` + profile `id` (and the rest of metadata).
- Calibration: `calibration_identity.format_version` and, if present, `calibration_identity.content_hash`. Together with profile id and profile format version, this identifies the exact calibration blob for reproduction.

**Use cases:**

1. **Re-run with same config** — Re-running a session or pipeline with the same profile and calibration (same version and, if stored, same content hash) so behavior is reproducible.
2. **Audit / share** — Others can verify or reuse the exact setup: they can check the profile and calibration identity (version and optional hash) and, with the same artifacts, reproduce the environment.

Identity is stored in profile metadata so that export (e.g. `export_profile`) and import (`import_profile`) preserve it; no extra files are required for reproducibility.
