---
phase: 05-hub-as-ide
plan: 05
subsystem: ui
tags: egui_dock, UiConfig, visualization, workbench, layout

# Dependency graph
requires:
  - phase: 05-hub-as-ide
    provides: Sidebar/nav (05-01), Visualization screen and layout
provides:
  - Visualization open anytime from sidebar; layout (preset + pane widgets) persisted via UiConfig
  - Run and Visualization usable together during experiments (content-area tabs when on Dashboard or Visualization)
affects: 05-06 (primary workflow)

# Tech tracking
tech-stack:
  added: []
  patterns: Content-area Run/Visualization tabs in Advanced mode; layout persistence in UiConfig

key-files:
  created: []
  modified:
    - crates/neurohid-hub/src/screens/visualization.rs
    - crates/neurohid-hub/src/layout.rs (existing persistence confirmed)
    - crates/neurohid-hub/src/app.rs
    - crates/neurohid-calibration/src/games/mod.rs

key-decisions:
  - "Run and Visualization together implemented as content-area tabs (Run | Visualization) when on Dashboard or Visualization in Advanced mode; no new lane or split pane."

patterns-established:
  - "Run/Visualization tabs: when current_screen is Dashboard or Visualization, show selectable Run | Visualization above content and sync workbench on switch."

requirements-completed: [HUB-04]

# Metrics
duration: ~25min
completed: 2026-02-21
---

# Phase 5 Plan 05: Visualization Customizable and Open with Run Summary

**Real-time signal and pipeline visualization in the Hub with customizable panels, layout persisted via UiConfig, and Run + Visualization open together via content-area tabs.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 2
- **Files modified:** 4 (hub: visualization.rs, app.rs; calibration: games/mod.rs; layout.rs confirmed)

## Accomplishments

- Confirmed Visualization is open anytime from the sidebar (top-level lane in Advanced mode) and supports add/remove/arrange panels via LayoutManager and egui_dock; layout preset and pane widget list persisted to UiConfig and restored on load.
- Run and Visualization can be used together during experiments: when on Dashboard or Visualization (Advanced mode), a Run | Visualization tab bar appears above the content so the user can switch without using the sidebar; workbench lane is synced on switch.

## Task Commits

Each task was committed atomically:

1. **Task 1: Ensure Visualization is customizable and open anytime** - `f423a32` (feat)
2. **Task 2: Run and Visualization open together** - `1c134a7` (feat)

## Files Created/Modified

- `crates/neurohid-hub/src/screens/visualization.rs` - Documented layout persistence via UiConfig and sidebar-open-anytime behavior
- `crates/neurohid-hub/src/app.rs` - Run | Visualization content-area tabs when on Dashboard or Visualization (Advanced mode)
- `crates/neurohid-calibration/src/games/mod.rs` - Added GameKind enum and all/display_name/description (blocking fix for hub build)
- `crates/neurohid-hub/src/layout.rs` - No code change; confirmed from_ui_config / take_persisted_state and UiConfig fields

## Decisions Made

- Run and Visualization "together" implemented as content-area tabs (Run | Visualization) when the current screen is Dashboard or Visualization in Advanced mode, rather than a new lane or side-by-side split. Minimal and matches "tabs or side-by-side" with tabs.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] GameKind missing in neurohid-calibration**
- **Found during:** Task 1 (cargo check -p neurohid-hub)
- **Issue:** hub depends on neurohid-calibration; panel.rs and hub calibration screen use GameKind, but games/mod.rs did not export it
- **Fix:** Added GameKind enum and impl with all(), display_name(), description() in crates/neurohid-calibration/src/games/mod.rs
- **Files modified:** crates/neurohid-calibration/src/games/mod.rs
- **Committed in:** f423a32 (Task 1 commit)

**Total deviations:** 1 auto-fixed (blocking)
**Impact on plan:** Unblocked hub build; no scope creep.

## Issues Encountered

None beyond the blocking GameKind export fix.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- HUB-04 satisfied: visualize real-time signal and pipeline state; customizable; Run and Visualization open together when needed.
- Ready for 05-06 (primary workflow: Run in Hub/background, resume state, docs).

## Self-Check

- SUMMARY.md created: yes
- Task commits present: f423a32, 1c134a7 (verified via git log)
- Key files exist and modified as listed: yes

---
*Phase: 05-hub-as-ide*
*Plan: 05*
*Completed: 2026-02-21*
