# Phase 1: Contracts and versioned formats - Research

**Researched:** 2026-02-20
**Domain:** Config/profile/stream format versioning, compatibility policy, stream semantics documentation, reproducibility identity
**Confidence:** HIGH

## Summary

Phase 1 adds no new runtime capabilities; it documents and versionizes existing formats so the same setup can be reproduced and compatibility is explicit. The codebase already uses Serde-based config (TOML) and profile metadata (JSON), and has precedent for versioned artifacts (ModelManifest: `model_version`, `feature_schema_version`, `action_schema_version`). Config and profile currently have no format version field—this phase adds a version and compatibility policy in the same doc as each format spec, supports reading at least N previous format versions (N to be chosen in planning), and documents stream consumption, timestamps, and "latest sample" semantics (e.g. drain-then-last for LSL). Calibration and profile metadata will store version/identity for reproducibility (re-run and audit/share). Research supports using the existing stack (serde, toml, serde_json), a single format-version field per artifact, and structured docs (BNF/schema plus brief prose) for stream semantics. LSL official docs confirm pull_sample return semantics (0.0 = no sample), timestamp meaning (remote capture time; map to local via time_correction), and buffer/drop behavior (max_buflen in seconds; overflow avoided by pulling or smaller buffer).

**Primary recommendation:** Add a `format_version` (or equivalent) to config and profile metadata; document version and compatibility policy in the same doc as the format spec; document LSL (and optionally other stream types) consumption model, timestamps, and latest-sample semantics in a dedicated stream-semantics doc; store reproducibility identity (version + optional content hash) in profile/calibration metadata so the same setup can be reproduced.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **Version and compatibility policy:** Compatibility policy lives in the same doc as the format spec (one place for version + rules). Until other people depend on this, breaking changes are acceptable; when they do, document breaks and migration. Support reading at least N previous format versions (N to be chosen during implementation/planning).

- **Reproducibility identity:** Use case for "same setup can be reproduced": both re-run with same config and audit/share (others can verify or reuse the exact setup).

- **Stream semantics documentation:** Document for streams: consumption model, timestamps, and ordering/drops (overflow/drop behavior). Documentation style: structured — BNF/schema plus brief prose.

### Claude's Discretion

- Versioning scheme for profile/config (e.g. semver vs single integer).
- What exactly identifies "same setup" (version-only vs version + content hash).
- Whether reproducibility applies to calibration, profile, or both in this phase.
- Where identity is stored (in-file, alongside, or both).
- Which stream types get documented semantics (LSL only vs all current stream types).
- Whether "latest sample" is defined once (generic) or per stream type.

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within phase scope.

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PATH-03 | Calibration and profile metadata are stored with version/identity so the same setup can be reproduced | Add format_version (and optional content hash) to profile metadata and calibration identity; document where stored (in-file vs manifest); doc recommends both re-run and audit/share use cases. |
| COMP-04 | Stream consumption, timestamp, and "latest sample" semantics are documented and consistent (e.g. drain-then-last for LSL) | LSL docs: pull_sample returns 0.0 when no sample; timestamps are remote capture time; drain-then-last = pull with timeout 0.0 until empty then use last. Document in structured form (schema + prose); extend to other stream types if in scope. |
| COMP-05 | Profile and config formats are versioned and have a documented compatibility policy | Add version field to config (TOML) and profile metadata (JSON); single doc per format containing version + compatibility rules; support reading N previous versions (N TBD in plan). |

</phase_requirements>

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| serde | 1.0 (workspace) | Serialization for config/profile | Already used; supports #[serde(default)] for additive evolution |
| toml | 0.9 (workspace) | Config file format | Already used in ConfigStore; roundtrip and pretty-print |
| serde_json | 1.0 (workspace) | Profile metadata and manifests | Already used for metadata.json, ModelManifest, IPC |

No new dependencies required for this phase. Version fields are plain data (e.g. `format_version: u32` or `format_version: String` for semver).

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| (none) | — | Schema/BNF for docs | Hand-written BNF or JSON Schema in docs; optional codegen later |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Integer format_version | Semver string | Integer: simpler parsing and comparison; semver: familiar and sortable. User left choice to discretion. |
| In-file version only | Version in separate manifest | In-file: one source of truth; manifest: allows read-only files. CONTEXT allows "in-file, alongside, or both." |

**Installation:** Not applicable — no new packages. Existing `neurohid-types`, `neurohid-storage` hold types and persistence.

## Architecture Patterns

### Recommended Project Structure

```
docs/
├── formats/
│   ├── config-format.md          # Config version + compatibility + (optional) schema
│   ├── profile-format.md        # Profile metadata version + compatibility + (optional) schema
│   └── stream-semantics.md      # Consumption, timestamps, latest-sample, per stream type
crates/neurohid-types/src/
├── config.rs                    # Add format_version to top-level or envelope
├── profile.rs                   # Add format_version to ProfileMetadata or envelope
```

Format and compatibility policy live in the same doc (per user decision). Stream semantics can live in one doc with sections per stream type (LSL, Serial, BrainFlow, Mock) or one section for LSL only if scope is LSL-only.

### Pattern 1: Version field at top level

**What:** Add a single `format_version` (or `schema_version`) field to the root struct serialized for config and profile metadata. Readers check the value and dispatch to the right deserializer or migration path.

**When to use:** Config and profile metadata; calibration identity (if stored as a small struct).

**Example:**

```rust
// Conceptual — config envelope
#[derive(Serialize, Deserialize)]
struct ConfigEnvelope {
    format_version: u32,  // e.g. 1
    config: SystemConfig,
}

// Profile metadata — add field to existing struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetadata {
    pub format_version: u32,  // NEW
    pub id: ProfileId,
    pub name: String,
    // ... existing fields
}
```

Existing code uses `SystemConfig` and `ProfileMetadata` directly; adding a version field is additive. Back-compat: if file has no version, treat as version 0 or 1 and apply defaults (already practiced in neurohid-types for observability, UiConfig).

### Pattern 2: Compatibility policy in same doc as format

**What:** One document per format (e.g. `docs/formats/config-format.md`) that defines the format version(s), the schema or BNF, and the compatibility policy (e.g. "Readers support format_version 1 and 2; 0 is unsupported" or "Breaking changes documented in CHANGELOG; N=2 supported").

**When to use:** Config format, profile format; referenced from README or docs index.

### Pattern 3: Stream semantics — consumption and timestamps

**What:** Structured section per stream type: consumption model (pull loop vs callback), meaning of timestamps (remote vs local, units), how "latest sample" is defined, and overflow/drop behavior (buffer size, what happens when buffer is full).

**When to use:** LSL (required for COMP-04); Serial, BrainFlow, Mock if scope includes "all current stream types."

**Example (LSL, from official docs):**

- **Consumption:** Blocking `pull_sample(timeout)` in a loop; samples delivered in order. Non-blocking: `pull_sample(0.0)` returns 0.0 when no sample available.
- **Timestamps:** Return value is capture time on remote machine (seconds, LSL clock); add `time_correction()` to map to local clock. NeuroHID currently converts to microseconds and stores in `Sample.device_timestamp` / `system_timestamp`.
- **Latest sample:** To get "most recent" sample only: repeatedly call `pull_sample(0.0)` until it returns 0.0 (drain), then use the last sample pulled (drain-then-last). Current NeuroHID LSL device does continuous pull (every 0.2s timeout), not drain-then-last; doc should state which behavior the runtime uses and how "latest" is defined for downstream.
- **Overflow/drops:** `max_buflen` in seconds (when nominal rate set); buffer > 0 required. If consumer does not pull fast enough, buffer fills; behavior (drop oldest vs block) is implementation-defined. Document NeuroHID's choice (e.g. 360s default, drop/block behavior from liblsl).

### Anti-Patterns to Avoid

- **Version in a separate repo or doc from the spec:** User required compatibility policy in the same doc as the format spec.
- **Documenting only "we use JSON/TOML" without version or schema:** PITFALLS.md already flags this; add version and, where useful, BNF or schema.
- **Defining "latest sample" only in code:** COMP-04 requires documented, consistent semantics (e.g. drain-then-last for LSL) so SDK/CLI users know what to expect.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Version parsing / comparison | Custom version DSL or multiple schemes in one codebase | Single scheme: one integer or one semver string per format | Consistency; simple reader logic; N-version support is "read v and v-1, v-2, ..." not a generic migration engine |
| Full migration framework | Generic migrator for all formats | Explicit N (e.g. N=2) and one or two migration paths in code | Phase scope is "documented and versioned"; complex migration can come later |
| Custom schema language | New format for schema | BNF or JSON Schema in docs; types in Rust as source of truth | Docs stay readable; Rust remains authority for runtime |

**Key insight:** The phase goal is clarity and reproducibility, not a generic versioning framework. Prefer minimal version field + one compatibility doc per format + structured stream-semantics doc.

## Common Pitfalls

### Pitfall 1: Version field omitted or inconsistent

**What goes wrong:** New code adds version to config but not profile, or version is in a different shape (e.g. string in config, int in profile), so compatibility policy is hard to state and readers diverge.

**Why it happens:** Incremental changes without a single "format version" contract per artifact type.

**How to avoid:** Define one version field name and type per format (e.g. `format_version: u32` for both config and profile); add it in one pass; document in the same doc as the compatibility policy.

**Warning signs:** PR adds version to only one of config/profile; version type differs between formats.

### Pitfall 2: LSL "latest sample" undefined or wrong

**What goes wrong:** Docs say "we use the latest sample" but runtime actually does continuous pull and uses every sample (or the opposite); SDK users assume drain-then-last and get different behavior.

**Why it happens:** "Latest" is ambiguous (last received vs last in buffer vs drain-then-last).

**How to avoid:** In stream-semantics doc, define "latest sample" explicitly for each stream type. For LSL, state whether NeuroHID uses drain-then-last or continuous pull and what "latest" means for the decoder/action path.

**Warning signs:** No sentence in docs that says how "latest sample" is obtained for LSL (or others).

### Pitfall 3: Reproducibility identity missing or not stored

**What goes wrong:** User exports a profile or config but there is no version or content hash; re-import or audit cannot verify "exact same setup."

**Why it happens:** Identity (version + optional hash) not added to metadata or calibration manifest.

**How to avoid:** Store format version in profile metadata and, if in scope, calibration identity (version + optional content hash) in the same place as calibration data (in-file or alongside). Document where identity lives and how to use it for re-run and audit/share.

**Warning signs:** Profile metadata or calibration has no version/identity field; export/import does not roundtrip identity.

## Code Examples

Verified patterns from the codebase and LSL docs:

### Config load/save (existing — add version on write and read)

Current pattern in `neurohid-storage/src/config.rs`: load TOML into `SystemConfig`, save with `toml::to_string_pretty`. Add a version field: either wrap in an envelope with `format_version` or add `format_version` to a new top-level config struct. On load, if version is missing, treat as legacy (e.g. version 1) and apply defaults.

### Profile metadata (existing — add version)

Current: `ProfileMetadata` in `neurohid-types/src/profile.rs` is serialized to JSON in `profile_metadata(&id)`. Add `format_version: u32` (or equivalent); on deserialize, use `#[serde(default)]` to default to 1 for existing files.

### LSL pull_sample and timestamp (existing — document semantics)

From `neurohid-device/src/lsl/device.rs` and liblsl docs:

- `inlet.0.pull_sample(0.2)` — blocking with 0.2s timeout; return value is remote capture time in seconds, or 0.0 if no sample.
- NeuroHID converts to micros: `device_ts = (timestamp * 1_000_000.0) as i64`, and sets `Sample { device_timestamp: Some(device_ts), system_timestamp: now_micros(), ... }`.
- No drain-then-last in current code; stream forwards every sample. Document this as "continuous pull" and define "latest sample" for consumers (e.g. "last sample in the current processing window" or "most recently received sample").

### ModelManifest versioning (existing precedent)

`neurohid-types/src/model.rs`: `ModelManifest` has `model_version`, `feature_schema_version`, `action_schema_version`, `trained_at`. `validate_runtime_compatibility()` checks schema versions against `CURRENT_*`. Reuse the same pattern for profile/config: version field + reader support for N previous versions.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|-------------|------------------|--------------|--------|
| No version in config/profile | Version field + compatibility doc in same place as spec | Phase 1 | Readers can detect format and support N versions; breaking changes documentable |
| Implicit stream semantics | Documented consumption, timestamps, latest-sample, overflow | Phase 1 | COMP-04 satisfied; SDK/CLI behavior predictable |
| No stored reproducibility identity | Version/identity in profile and optionally calibration | Phase 1 | PATH-03 satisfied; re-run and audit/share possible |

**Deprecated/outdated:** Relying on "we use TOML/JSON" without a version or compatibility policy (called out in `.planning/research/PITFALLS.md`).

## Open Questions

1. **N (number of previous format versions to support)**  
   - What we know: User said "N to be chosen during implementation/planning."  
   - What's unclear: Concrete N for config and for profile (could differ).  
   - Recommendation: Plan with N=2; implement one migration path (v0/v1 → current) if needed.

2. **Versioning scheme (integer vs semver)**  
   - What we know: User left to discretion.  
   - What's unclear: Whether config/profile should use the same scheme as ModelManifest (which has string `model_version` and integer schema versions).  
   - Recommendation: Use a single integer `format_version` for config and profile for simplicity; keep model_version as-is for manifests.

3. **Which stream types to document**  
   - What we know: CONTEXT says "LSL only vs all current stream types" is discretion.  
   - What's unclear: Whether Serial/BrainFlow/Mock need full semantics sections or a short "same as LSL where applicable" note.  
   - Recommendation: Document LSL in full; add short subsections for Serial/BrainFlow/Mock (consumption model, timestamps, latest-sample if applicable) to keep one stream-semantics doc complete.

4. **Where to store calibration identity**  
   - What we know: Identity can be in-file, alongside, or both.  
   - What's unclear: Whether calibration is a separate artifact with its own version or only profile metadata carries identity.  
   - Recommendation: Store profile (and optionally calibration) format version and identity in profile metadata and, if needed, a small calibration manifest alongside calibration.enc so the same setup is reproducible.

## Sources

### Primary (HIGH confidence)

- Lab Streaming Layer liblsl 1.13 Stream Inlets (pull_sample, timestamps, max_buflen): https://labstreaminglayer.readthedocs.io/projects/liblsl/ref/inlet.html
- NeuroHID codebase: `crates/neurohid-types/src/config.rs`, `profile.rs`, `model.rs`; `crates/neurohid-storage/src/config.rs`, `profile.rs`, `paths.rs`; `crates/neurohid-device/src/lsl/device.rs`, `lsl/provider.rs`; `docs/architecture-rust-core.md`; `docs/protocol-and-api.md`
- Project planning: `.planning/phases/01-contracts-and-versioned-formats/01-CONTEXT.md`, `.planning/REQUIREMENTS.md`, `.planning/research/PITFALLS.md`

### Secondary (MEDIUM confidence)

- Web search: LSL pull_sample timestamp semantics, buffer overflow, drain-then-last (verified against official inlet docs above)

### Tertiary (LOW confidence)

- None

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH — existing crates and serialization; no new libs.
- Architecture: HIGH — version field and same-doc policy are standard; stream semantics from LSL docs and code.
- Pitfalls: HIGH — PITFALLS.md and codebase confirm unversioned config/profile and missing stream semantics.

**Research date:** 2026-02-20  
**Valid until:** ~30 days (formats and LSL semantics are stable; only N and scheme choices may change during planning).
