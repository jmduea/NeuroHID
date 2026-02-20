# Phase 4: Standard path and recording - Research

**Researched:** 2026-02-20
**Domain:** Documented user path, session recording, XDF/export formats, replay
**Confidence:** MEDIUM (XDF/spec and codebase verified; replay and writer options from multiple sources; doc placement aligned with existing docs IA)

## Summary

Phase 4 delivers (1) one coherent documented path from device-in-hand to decoder-driving-actions, and (2) session recording with export to XDF (primary) and at least one secondary format (e.g. JSON trace for actions), plus replay and reproducibility.

**Standard path:** Add a user-facing doc (e.g. "Getting started" or "User guide") as a section or dedicated doc linked from `docs/index.md`. The path is a single walkthrough with optional branches ("if you use X, do this; else that"), informal tone, minimal assumptions (show how to pick device and decoder, then run). Existing `docs/development-guide.md` and `docs/deployment-guide.md` stay canonical for build and ops; the new content is the one path for end-users and developers.

**Recording:** Full session = raw streams + config (profile, decoder params, pipeline) + decoder output/actions. Session folder with separate files by default (config copy, stream data, actions trace); configurable single-file or split/export later. Start/stop: both optional auto (tied to runtime or output) and explicit; feedback in Hub/CLI and/or log. On failure: stop recording, keep what was written, surface error. Optional cap (max duration/size) in config.

**Export/replay:** XDF is the primary interchange format (multi-stream, timestamps, metadata; ecosystem support in MATLAB, EEGLAB, Python, MNE). The Rust `xdf` crate is read-only; writing XDF requires implementing the published spec or using LSL + Lab Recorder when the pipeline uses LSL. Recommend: record to a session folder (native layout: config + raw streams + actions), then "export to XDF" (Rust writer implemented against [XDF spec](https://github.com/sccn/xdf/wiki/Specifications)) for primary format; JSON trace for actions as secondary. Replay = (1) offline run of decoder on recorded data (validation) and (2) feed recording as virtual live source (mock device or LSL outlet from file).

**Reproducibility:** Captured in the session/export: full profile + decoder params + device type and stream choices + runtime/SDK/format version so a third party can reproduce. No separate "reproducibility" subsection in docs—reproducibility is a side effect of recording/export.

**Primary recommendation:** Implement session recording to a session folder (config snapshot, raw stream capture, actions trace); add an XDF writer in Rust against the XDF 1.0 spec for primary export; document the single path in a new user-facing section or doc; support replay via a file-backed/virtual source and offline decoder run.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Documented path shape**
- Tutorial/walkthrough with optional branches (e.g. "if you use X, do this; else that").
- Audience: both end-user and developer — one path with optional "advanced" asides.
- Location: section inside a larger "Getting started" or "User guide" (not a standalone doc).
- Defaults: minimal assumptions — path shows how to pick device and decoder, then run.
- Tone: informal.

**Recording scope and triggers**
- **Content:** Full session trace — raw streams + config (profile, decoder params, pipeline) + decoder output/actions.
- **Start/stop:** Both — optional auto (tied to runtime) and explicit start/stop. When auto: configurable — user picks "tied to output" vs "tied to runtime".
- **Where:** Configurable default in config/profile; override per session allowed.
- **Feedback:** Visible indicator (Hub/CLI) and/or log when recording starts/stops.
- **File layout:** Session folder with separate files by default; configurable option for single file or split/export.
- **On failure:** Stop recording, keep what was written, surface error to user/API.
- **Cap:** Optional — default no cap; config can set max duration and/or size.

**Export/replay format and tooling**
- **Formats:** XDF primary; document conversion/interop with other formats; support at least one other format (e.g. EDF or simple CSV/JSON trace) for analysis. Start with one secondary (e.g. JSON trace for actions), leave room for more later.
- **Replay:** Both — (1) replay into decoder: offline run on recording (validation) and feed recording as virtual live source; (2) export for external tools.
- **Invocation:** Default save in standard format; optional "export" for conversion or extra formats. Export available from both Hub and CLI/SDK.
- **External tools:** Files that open in common tools + documented layout; optional export presets (e.g. "for EEGLAB" vs "for custom analysis").

**Reproducibility story**
- **Meaning:** Both — deterministic replay when possible, plus enough metadata that a third party can reproduce the setup.
- **Captured:** Full profile + decoder params + device type and stream choices, plus version/identity of runtime, SDK, or format so tool versions are known.
- **In docs:** No separate "reproducibility" subsection — reproducibility is a side effect of recording/export.

### Claude's Discretion
- What exactly to include in export so someone can reproduce or analyze (e.g. data-only vs data+config vs manifest).
- How to express "this recording matches this config" (bundled config, hash/id to stored profile, or human-readable manifest).

### Deferred Ideas (OUT OF SCOPE)
- None — discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PATH-01 | User can follow documented steps from "device in hand" to "decoder driving actions" using defaults and one coherent path | Standard path as section in Getting started/User guide; docs IA (index.md, deployment-guide) and CONTEXT (location, tone, audience) define placement and style. |
| PATH-02 | User can record/export sessions and replay or analyze them in standard formats (e.g. XDF, documented config) for reproducibility | XDF spec and ecosystem; session folder layout (config + streams + actions); Rust `xdf` read-only → implement writer or session→XDF export; replay as virtual source + offline decoder run; reproducibility via captured config/profile/version. |
</phase_requirements>

## Standard Stack

### Core

| Library / artifact | Version / ref | Purpose | Why standard |
|-------------------|---------------|---------|--------------|
| XDF (format) | 1.0 (beta) | Primary session/export format for multi-stream time series + metadata | LSL ecosystem standard; MATLAB, EEGLAB, Python (pyxdf, MNE), BCILAB; open spec (sccn/xdf wiki). |
| Session folder (layout) | N/A | Default recording layout: config + stream files + actions trace | CONTEXT: "session folder with separate files by default"; enables incremental write, simple replay, and export to XDF. |
| JSON (actions trace) | N/A | Secondary format for decoder output / actions | CONTEXT: "e.g. JSON trace for actions"; simple, tool-friendly, one stream type. |

### Supporting

| Library / artifact | Version / ref | Purpose | When to use |
|-------------------|---------------|---------|-------------|
| pyxdf | 1.17+ (Python 3.9+) | Read XDF in Python | Replay/analysis in Python, validation, conversion. |
| Rust `xdf` crate | 0.1.x | Read XDF in Rust | Replay or validation in Rust; **write not implemented** — use spec to implement writer. |
| Lab Recorder (LSL) | N/A | Record LSL streams to XDF | When pipeline uses LSL and recording is done externally; not in-process NeuroHID recording. |
| MNE-Python | N/A | Read XDF (e.g. `mne.io.read_raw_xdf`) | External analysis; document in "opens in common tools". |

### Alternatives considered

| Instead of | Could use | Tradeoff |
|------------|-----------|----------|
| Native XDF writer in Rust | LSL outlet + Lab Recorder | LSL path requires LSL in pipeline and external process; native writer gives single-process recording and export without LSL. |
| Single monolithic recording file | Session folder + export to XDF | Folder allows incremental write and simple per-stream files; XDF export satisfies "primary format" and external tools. |

**Installation (relevant):**
- Python (pyxdf): `uv add pyxdf` in `python/` or `pip install pyxdf`.
- Rust: add `xdf` crate for read path; writer implemented in-tree against [XDF Specifications](https://github.com/sccn/xdf/wiki/Specifications).

## Architecture Patterns

### Recommended recording layout (session folder)

```
session_<id>/
├── manifest.json       # session_id, start/end, config_ref, format_version, runtime/SDK version
├── config.yaml         # Snapshot of SystemConfig (or path + hash to stored config)
├── profile_meta.json   # ProfileMetadata snapshot (or ref) for reproducibility
├── streams/            # Raw streams (e.g. one file per stream or one combined)
│   └── ...
├── actions.jsonl       # Decoder output / actions (one JSON object per line)
└── (optional) recording.xdf  # Or produced only on "export to XDF"
```

- **Manifest:** Session identity, time range, link to config (path or embedded snapshot), tool versions. Supports "this recording matches this config" (Claude's discretion: manifest can reference profile id + config path, or embed hash).
- **Config + profile:** Copy or reference; for reproducibility third party needs full profile + decoder params + device/stream choices (CONTEXT).
- **Streams:** Raw samples (e.g. neurohid_types::signal::Sample serialized or written in a simple binary/CSV per stream); export step can merge into XDF streams.
- **Actions:** JSONL for decoder output (timestamp, decision, optional confidence) — secondary format, easy to parse.

### Pattern 1: Recording tap

**What:** A recording component receives samples (and optionally config snapshots and decoder output) and writes to the session folder. It does not block the main pipeline.

**When to use:** Whenever recording is enabled (auto or explicit start).

**Implementation note:** Runtime already has `sample_tx` (device → signal) and `sample_broadcast_tx` for Hub. Recording can subscribe to a broadcast clone or a dedicated tap from the device/signal boundary; decoder output from the decoder task. Start/stop via control API and/or config (auto tied to runtime or output). On failure: stop writing, flush, surface error (CONTEXT).

### Pattern 2: Export to XDF

**What:** Convert session folder (streams + metadata) into one or more XDF files. XDF 1.0 = FileHeader + StreamHeader (XML) + Samples + ClockOffset + StreamFooter; little-endian, chunked (spec: [sccn/xdf wiki](https://github.com/sccn/xdf/wiki/Specifications)).

**When to use:** "Save in standard format" (default) or "export" for external tools (EEGLAB, MNE, etc.).

**Options:** (1) Implement XDF writer in Rust following the spec (recommended for single-process, no LSL dependency). (2) If LSL is used, optionally stream to LSL and let Lab Recorder write XDF (separate process). Phase 4 can start with session folder + JSON trace as secondary; XDF writer in Rust satisfies primary format and keeps tooling in-tree.

### Pattern 3: Replay as virtual source

**What:** Feed recorded data into the pipeline as if from a device: either (1) offline run — load session, run decoder on recorded streams, compare to recorded actions; or (2) virtual live source — a mock or file-backed device that yields samples from the recording (with optional timing) so the same decoder runs on replayed data.

**When to use:** Validation, debugging, reproducible demos.

**Implementation note:** Device task today takes a device backend; a "replay" or "file" backend that reads from session folder or XDF and implements the same sample-producing interface allows "feed recording as virtual live source". Offline run can be a CLI or Python script: load XDF (pyxdf or Rust `xdf`), run decoder (or call into runtime in batch mode), output actions for comparison.

### Pattern 4: Standard path doc placement

**What:** One coherent path (device → pick decoder → run) as a section inside a "Getting started" or "User guide" doc, linked from `docs/index.md`. Informal tone, optional branches, minimal assumptions.

**When to use:** PATH-01; first-time and recurring users.

**Placement:** `docs/index.md` currently lists development-guide, deployment-guide, architecture, protocol. Add a user-facing entry (e.g. "User guide" or "Getting started") that contains the standard path section; do not duplicate long runbooks from deployment-guide — link to it where appropriate.

### Anti-patterns to avoid

- **Treating pyxdf as writer:** pyxdf only reads XDF; do not assume a `save_xdf()`.
- **One giant file only:** CONTEXT says session folder with separate files by default; single file is an option, not the only layout.
- **Skipping config/profile in recording:** Reproducibility requires full profile + decoder params + device/stream choices + version (CONTEXT).
- **Separate "reproducibility" doc:** Reproducibility is a side effect of recording/export; no dedicated subsection (CONTEXT).

## Don't Hand-Roll

| Problem | Don't build | Use instead | Why |
|---------|-------------|-------------|-----|
| Multi-stream time-series container | Custom binary format | XDF (spec) or session folder + export to XDF | XDF is standard; spec is fixed; ecosystem support. |
| Timestamp sync across streams | Ad-hoc offsets | XDF ClockOffset chunks (when writing XDF) | Spec defines sync; importers expect it. |
| Config/profile snapshot for reproducibility | Undocumented freeform | Versioned SystemConfig + ProfileMetadata (existing formats) | config-format.md and profile-format.md already define versioning and identity. |
| Actions / decoder output trace | Custom binary | JSONL or JSON | Simple, one format for "at least one other format"; tool-friendly. |

**Key insight:** XDF and versioned config/profile already exist; recording should emit data and metadata in these shapes rather than inventing new container formats.

## Common Pitfalls

### Pitfall 1: Assuming pyxdf or Rust `xdf` writes XDF

**What goes wrong:** Design assumes `save_xdf()` or `XDFFile::write()` exists; it does not in pyxdf or the current Rust crate.

**Why it happens:** Both libraries are read-focused; write is "maybe one day" in Rust, not present in pyxdf.

**How to avoid:** Plan for implementing an XDF writer against the spec (Rust) or using LSL + Lab Recorder when LSL is in use; for session storage use session folder + optional export-to-XDF step.

**Warning signs:** Task says "use pyxdf to save" or "use xdf crate to write".

### Pitfall 2: Recording blocks the pipeline

**What goes wrong:** Recording runs on the same thread or blocks sample_tx; latency or drops increase.

**Why it happens:** Synchronous write or single channel consumer.

**How to avoid:** Recording as separate task; receive samples via broadcast or dedicated channel; write async or buffered; backpressure or drop policy documented (CONTEXT: on failure stop, keep written, surface error).

**Warning signs:** Recording code in device or signal task hot path.

### Pitfall 3: Missing reproducibility metadata

**What goes wrong:** Export has raw data but no profile/decoder/device/version; third party cannot reproduce.

**Why it happens:** Focusing only on "streams" and forgetting CONTEXT ("full profile + decoder params + device type and stream choices + version/identity").

**How to avoid:** Manifest or session metadata always includes (or references) config snapshot, profile identity, decoder params, device/stream info, runtime/SDK/format version.

**Warning signs:** Session folder has only `streams/` and `actions.jsonl` with no config or manifest.

### Pitfall 4: Standard path doc in wrong place or tone

**What goes wrong:** Path is buried in deployment-guide as a runbook, or tone is formal.

**Why it happens:** Reusing ops doc for first-run experience.

**How to avoid:** Dedicated section in a user-facing doc (Getting started / User guide), informal tone, one path with optional branches; link to deployment-guide for transport/control details.

**Warning signs:** No single "start here" entry in docs/index for the path; path written in imperative/formal style.

## Code Examples

### NeuroHID Sample type (for recording streams)

```rust
// Source: crates/neurohid-types/src/signal.rs
// Recording writes samples (or equivalent) per stream; export maps to XDF stream format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub source_id: Option<String>,
    pub device_timestamp: Option<Timestamp>,
    pub system_timestamp: Timestamp,
    pub sequence_number: Option<u64>,
    pub values: Vec<f32>,
    pub quality: Option<Vec<f32>>,
}
```

### XDF StreamHeader (minimal) from spec

```xml
<?xml version="1.0"?>
<info>
    <name>NeuroHID-EEG</name>
    <type>EEG</type>
    <channel_count>8</channel_count>
    <nominal_srate>100</nominal_srate>
    <channel_format>float32</channel_format>
    <version>1</version>
    <source_id>...</source_id>
    <created_at>...</created_at>
    <uid>...</uid>
    <session_id>...</session_id>
</info>
```

Source: [XDF Specifications - StreamHeader](https://github.com/sccn/xdf/wiki/Specifications)

### Reading XDF in Python (pyxdf)

```python
# Source: pyxdf docs / ecosystem
import pyxdf
streams, header = pyxdf.load_xdf("recording.xdf")
# streams: list of dicts with 'time_series', 'time_stamps', 'info', etc.
```

### Reading XDF in Rust (xdf crate)

```rust
// Source: docs.rs/xdf
let bytes = std::fs::read("recording.xdf")?;
let xdf_file = xdf::XDFFile::from_bytes(&bytes)?;
// xdf_file contains streams; write path not in crate — implement per spec.
```

## State of the Art

| Old approach | Current approach | When changed | Impact |
|--------------|------------------|--------------|--------|
| Ad-hoc lab formats | XDF 1.0 as LSL/ecosystem standard | XDF spec stable (sccn/xdf) | Use XDF for interchange; session folder for internal recording is fine, export to XDF for sharing. |
| pyxdf read-only | Same | N/A | Writing XDF: implement from spec or use Lab Recorder (LSL). |
| Rust xdf crate read-only | Same (0.1.x) | N/A | Writer not in crate; implement in-tree if needed. |

**Deprecated/outdated:** None relevant; XDF 1.0 (beta) is the current spec.

## Open Questions

1. **Manifest: bundled config vs reference**
   - What we know: CONTEXT requires full profile + decoder params + device/stream + version; Claude's discretion = how to express "this recording matches this config."
   - What's unclear: Whether manifest should embed full config YAML or only path + hash to stored config.
   - Recommendation: Start with embedded config snapshot (or path + hash) in manifest so export is self-contained; allow optional "reference only" for smaller size.

2. **XDF writer: in-tree Rust vs Python**
   - What we know: Spec is clear; Rust crate doesn't write; pyxdf doesn't write.
   - What's unclear: Whether to prioritize a Rust writer (single binary, no Python for export) or a small Python script using a custom/spec-based writer.
   - Recommendation: Implement XDF writer in Rust against spec so export is available from CLI/SDK without Python; Python remains for analysis/replay (pyxdf load_xdf).

## Sources

### Primary (HIGH confidence)

- [XDF Specifications (sccn/xdf wiki)](https://github.com/sccn/xdf/wiki/Specifications) — chunk types, StreamHeader XML, Samples, ClockOffset, file structure.
- NeuroHID codebase: `crates/neurohid-types/src/signal.rs` (Sample), `crates/neurohid-core/src/tasks/session_logger.rs` (existing session log for training only), `crates/neurohid-core/src/service.rs` (sample_tx, sample_broadcast_tx), `docs/formats/config-format.md`, `docs/formats/profile-format.md`, `docs/formats/stream-semantics.md`.
- CONTEXT.md and REQUIREMENTS.md (PATH-01, PATH-02).

### Secondary (MEDIUM confidence)

- docs.rs/xdf — Rust crate read-only, no write.
- PyPI pyxdf, Lab Recorder (LSL→XDF), MNE read_raw_xdf — read/record ecosystem; pyxdf write not present (verified via search).
- `.planning/research/ARCHITECTURE.md` — LSL, Lab Recorder, XDF mentioned; recording as optional.

### Tertiary (LOW confidence)

- Web search: "pyxdf write" (confirms read-only); "LSL playback replay XDF" (replay = load then push to pipeline or LSL outlet).

## Metadata

**Confidence breakdown:**
- Standard stack: MEDIUM — XDF spec and codebase verified; writer choice (implement vs LSL) from spec + crate/docs.
- Architecture: MEDIUM — session folder and tap pattern align with CONTEXT and existing service.rs; replay patterns from architecture doc and replay use cases.
- Pitfalls: MEDIUM — pyxdf/xdf write limitation verified; others from CONTEXT and common design mistakes.

**Research date:** 2026-02-20  
**Valid until:** ~30 days for format/spec; re-check writer availability (Rust xdf crate) if implementing writer in-tree.
