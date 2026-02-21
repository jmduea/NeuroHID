---
phase: 05-hub-as-ide
plan: 06
subsystem: ui
tags: hub, egui, config, resume, runtime-mode, user-guide

# Dependency graph
requires:
  - phase: 05-hub-as-ide
    provides: Hub lanes, Training screen, Visualization, Devices, Calibration
provides:
  - Resume state (last_screen) so Hub reopens to last open view
  - Run in Hub vs Run in background explicit in UI (Settings + Dashboard)
  - Primary workflow documented in user-guide and suggested in Dashboard
affects: Phase 5 Hub-as-IDE, user onboarding

# Tech tracking
tech-stack:
  added: []
  patterns: UiConfig resume field; Screen id/from_id for stable persistence; ServiceRuntimeMode::ui_label for user-facing labels

key-files:
  created: []
  modified:
    - crates/neurohid-types/src/config.rs
    - crates/neurohid-hub/src/app.rs
    - crates/neurohid-hub/src/state.rs
    - crates/neurohid-hub/src/screens/mod.rs
    - crates/neurohid-hub/src/screens/dashboard.rs
    - crates/neurohid-hub/src/screens/settings.rs
    - docs/user-guide.md

key-decisions:
  - "Resume state stored as last_screen (string ID) in UiConfig; applied on startup when valid for current UI mode"
  - "Run in Hub / Run in background as user-facing labels; ServiceRuntimeMode::ui_label() for consistency"

patterns-established:
  - "Persist last_screen on every screen change via persist_last_screen() and existing config save path"
  - "Hub workflow documented in user-guide; optional collapsible 'Suggested path' on Dashboard"

requirements-completed: [HUB-05]

# Metrics
duration: ~15min
completed: 2026-02-21
---

# Phase 5 Plan 6: Primary Workflow (Run in Hub/background, resume state) Summary

**Resume state (last screen restored on reopen), Run in Hub vs Run in background explicit in UI, and primary workflow (Devices → Calibration → Training → Run) documented and suggested in the Hub.**

## Performance

- **Duration:** ~15 min
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments

- UiConfig.last_screen added with serde default; Hub restores current_screen on startup when valid for mode.
- Screen::id() / Screen::from_id() for stable persistence; last_screen persisted on every screen change.
- ServiceRuntimeMode::ui_label() ("Run in Hub" / "Run in background"); Settings Run dropdown and Dashboard use these labels.
- User guide: "Hub workflow: standard path in the Hub" section; Dashboard collapsible "Suggested path" hint.

## Task Commits

1. **Task 1: Add resume state to UiConfig and apply on startup** - `795c23b` (feat)
2. **Task 2: Run in Hub vs Run in background explicit in UI** - `b6af319` (feat)
3. **Task 3: Primary workflow documented or suggested in UI** - `5d6ab23` (docs)

## Files Created/Modified

- `crates/neurohid-types/src/config.rs` - UiConfig.last_screen, ServiceRuntimeMode::ui_label(); backcompat test
- `crates/neurohid-hub/src/app.rs` - Restore screen on startup, persist_last_screen(), call sites
- `crates/neurohid-hub/src/screens/mod.rs` - Screen::id(), Screen::from_id()
- `crates/neurohid-hub/src/screens/dashboard.rs` - ui_label() for Diagnostics chip; start button labels; "Suggested path" collapsing hint
- `crates/neurohid-hub/src/screens/settings.rs` - Run dropdown and chips use "Run in Hub" / "Run in background"
- `docs/user-guide.md` - "Hub workflow: standard path in the Hub" section

## Decisions Made

- Resume stored as optional string ID (last_screen) in UiConfig; no new persistence layer.
- User-facing labels centralized in ServiceRuntimeMode::ui_label() for Settings and Dashboard.

## Deviations from Plan

None - plan executed as written.

## Issues Encountered

- neurohid-types test target fails to compile due to pre-existing ControlSnapshot initializer in ipc.rs (missing fields); scope limited to 05-06; cargo check --workspace passes.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- HUB-05 satisfied: one primary workflow without switching tools; Run choice and resume state implemented.
- Phase 5 (Hub-as-IDE) plan 06 complete; next is Phase 6 or remaining phase 5 follow-ups per roadmap.

## Self-Check: PASSED

- 05-06-SUMMARY.md present
- Commits 795c23b, b6af319, 5d6ab23 verified in git log

---
*Phase: 05-hub-as-ide*
*Completed: 2026-02-21*
