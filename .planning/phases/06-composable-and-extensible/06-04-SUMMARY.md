---
phase: 06-composable-and-extensible
plan: 04
subsystem: ui
tags: [extensions, hub, cli, egui, neurohid-core]

# Dependency graph
requires:
  - phase: 06-01
    provides: Extension registry, discovery, default path
  - phase: 06-02
    provides: Name-based config (device/outlet/signal/decoder extension_name), factories
provides:
  - Hub Extensions screen (list + rescan) and device/signal/decoder/outlet selection with immediate config sync
  - CLI neurohid extensions list | refresh; discovery path and CLI documented
affects: [06-03 example plugin visibility, user docs]

# Tech tracking
tech-stack:
  added: []
  patterns: [Extension registry in Hub for dropdowns and Extensions screen; CLI handling in neurohid binary before service dispatch]

key-files:
  created: [crates/neurohid-hub/src/screens/extensions.rs]
  modified: [crates/neurohid-hub/src/screens/mod.rs, settings.rs, app.rs, workbench.rs, crates/neurohid/src/bin/neurohid.rs, docs/extension-contracts.md]

key-decisions:
  - "Extensions CLI handled in neurohid (GUI) binary: extensions list/refresh run in-process and exit; no service subcommand"
  - "Slot selection (device/signal/decoder/outlet) persists immediately after section render when value changed (per CONTEXT)"

patterns-established:
  - "Extensions screen: scan on first show and on Rescan; same UI when empty (shorter list)"
  - "Settings dropdowns: built-in + extension names from registry; persist on change after collapsing header"

requirements-completed: [COMP-06]

# Metrics
duration: 25min
completed: "2026-02-21"
---

# Phase 06 Plan 04: Hub and CLI Extensions Parity Summary

**Hub Extensions screen with list/rescan, device/signal/decoder/outlet dropdowns with immediate config sync; CLI extensions list/refresh and discovery docs.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 2
- **Files modified:** 6 (5 modified, 1 created)

## Accomplishments

- Extensions screen in Hub: lists discovered outlet, device, signal preprocessing, and decoder extensions; Rescan button; same UI when none (shorter list).
- Device backend dropdown in Settings: built-in (Auto, LSL, Mock, Serial, BrainFlow) plus discovered device extension names; selection syncs to config immediately.
- Signal, Decoder, and Outlet dropdowns in Settings: Built-in + extension by name; selection writes config and persists immediately.
- CLI: `neurohid extensions list` and `neurohid extensions refresh` scan default path, print kind/name/path (tab-separated), exit 0 on success and non-zero on discovery failure.
- docs/extension-contracts.md: CLI usage, discovery path, and "Adding an extension" section.

## Task Commits

1. **Task 1: Hub Extensions screen and device/signal/decoder/outlet dropdowns** - `44297ef` (feat)
2. **Task 2: CLI extensions list/refresh and docs** - `1dc66a2` (feat)

## Files Created/Modified

- `crates/neurohid-hub/src/screens/extensions.rs` - New Extensions screen (list + rescan)
- `crates/neurohid-hub/src/screens/mod.rs` - Screen::Extensions, id/label/from_id/all_for_mode
- `crates/neurohid-hub/src/screens/settings.rs` - Device backend + signal/decoder/outlet extension dropdowns; immediate persist on slot change
- `crates/neurohid-hub/src/app.rs` - Extensions screen instance, match arm, glyph, command palette
- `crates/neurohid-hub/src/workbench.rs` - Extensions in Config lane; lane_for_screen
- `crates/neurohid/src/bin/neurohid.rs` - extensions list/refresh CLI; run_extensions_cli
- `docs/extension-contracts.md` - CLI subsection, Adding an extension section

## Decisions Made

- Extensions CLI implemented in the neurohid (GUI) binary so list/refresh run without starting the service or GUI; discovery uses same default path as core.
- Slot selection persist is done after each Settings section when the slot value changed (compare before/after section) to avoid borrow conflicts.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- Borrow checker: persisting inside the Settings collapsing header closure (with `cfg = &mut state.config.device` etc.) required moving persist to after each section and comparing last_* to current value.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- COMP-06 satisfied: Hub and CLI parity for extensions; user can manage and choose extensions for all four slots from Hub and headless.
- Example plugin from 06-03 can be rescanned and selected in dropdowns; discovery path and CLI documented.

## Self-Check: PASSED

- `crates/neurohid-hub/src/screens/extensions.rs` present
- `06-04-SUMMARY.md` present
- Commits `44297ef`, `1dc66a2` present

---
*Phase: 06-composable-and-extensible*
*Completed: 2026-02-21*
