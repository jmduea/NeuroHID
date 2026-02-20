# Phase 4: Standard path and recording - Context

**Gathered:** 2026-02-20
**Status:** Ready for planning

<domain>
## Phase Boundary

One coherent path from "device in hand" to "decoder driving actions" (documented steps, minimal assumptions), and the ability to record, export, and replay sessions in standard formats so setups are reproducible. Discussion clarified how the path is presented, what is recorded, how export/replay work, and what "reproducible" means. New capabilities (e.g. extra formats, separate reproducibility doc) stay in scope only where agreed.
</domain>

<decisions>
## Implementation Decisions

### Documented path shape
- Tutorial/walkthrough with optional branches (e.g. "if you use X, do this; else that").
- Audience: both end-user and developer — one path with optional "advanced" asides.
- Location: section inside a larger "Getting started" or "User guide" (not a standalone doc).
- Defaults: minimal assumptions — path shows how to pick device and decoder, then run.
- Tone: informal.

### Recording scope and triggers
- **Content:** Full session trace — raw streams + config (profile, decoder params, pipeline) + decoder output/actions.
- **Start/stop:** Both — optional auto (tied to runtime) and explicit start/stop. When auto: configurable — user picks "tied to output" vs "tied to runtime".
- **Where:** Configurable default in config/profile; override per session allowed.
- **Feedback:** Visible indicator (Hub/CLI) and/or log when recording starts/stops.
- **File layout:** Session folder with separate files by default; configurable option for single file or split/export.
- **On failure:** Stop recording, keep what was written, surface error to user/API.
- **Cap:** Optional — default no cap; config can set max duration and/or size.

### Export/replay format and tooling
- **Formats:** XDF primary; document conversion/interop with other formats; support at least one other format (e.g. EDF or simple CSV/JSON trace) for analysis. Start with one secondary (e.g. JSON trace for actions), leave room for more later.
- **Replay:** Both — (1) replay into decoder: offline run on recording (validation) and feed recording as virtual live source; (2) export for external tools.
- **Invocation:** Default save in standard format; optional "export" for conversion or extra formats. Export available from both Hub and CLI/SDK.
- **External tools:** Files that open in common tools + documented layout; optional export presets (e.g. "for EEGLAB" vs "for custom analysis").

### Reproducibility story
- **Meaning:** Both — deterministic replay when possible, plus enough metadata that a third party can reproduce the setup.
- **Captured:** Full profile + decoder params + device type and stream choices, plus version/identity of runtime, SDK, or format so tool versions are known.
- **In docs:** No separate "reproducibility" subsection — reproducibility is a side effect of recording/export.

### Claude's Discretion
- What exactly to include in export so someone can reproduce or analyze (e.g. data-only vs data+config vs manifest).
- How to express "this recording matches this config" (bundled config, hash/id to stored profile, or human-readable manifest).
</decisions>

<specifics>
## Specific Ideas

- Tone for the standard-path section: informal.
- No other specific references — open to standard approaches for layout and tooling.
</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.
</deferred>

---

*Phase: 04-standard-path-and-recording*
*Context gathered: 2026-02-20*
