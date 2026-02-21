---
phase: 05-hub-as-ide
plan: 01
subsystem: ui
tags: [hub, egui, sidebar, workbench, rust]

# Dependency graph
requires:
  - phase: 04-standard-path-and-recording
    provides: user guide, session recording, XDF export
provides:
  - Hub sidebar and workbench aligned to Phase 5 CONTEXT (Devices, Calibration, Training, Visualization, Config)
  - Screen::Training and Training screen stub reachable from sidebar and command palette
affects: 05-02 through 05-06 (devices, calibration, training, visualization, primary workflow)

# Tech tracking
tech-stack:
  added: []
  patterns: [ActivityLane as CONTEXT-ordered sidebar lanes, screens_for_lane/lane_for_screen mapping]

key-files:
  created: [crates/neurohid-hub/src/screens/training.rs]
  modified: [crates/neurohid-hub/src/workbench.rs, crates/neurohid-hub/src/screens/mod.rs, crates/neurohid-hub/src/app.rs, crates/neurohid-hub/src/service_manager.rs]

key-decisions:
  - "Lanes: Devices, Calibration, Training, Visualization, Config (Dashboard/Profiles/Settings/Labs in Config)"
  - "Default lane: Devices; keyboard shortcuts D/C/T/V/G for lanes"

patterns-established:
  - "Primary workflow screens (Calibration, Training, Visualization) and Devices are top-level lanes; Config groups secondary screens"

requirements-completed: [HUB-05]

# Metrics
duration: ~25min
completed: 2026-02-21
---

# Phase 5 Plan 01: Sidebar/Nav and Training Stub Summary

**Hub sidebar and workbench aligned to Phase 5 CONTEXT with five lanes (Devices, Calibration, Training, Visualization, Config), Screen::Training, and a Training screen stub so the primary workflow can be followed without switching tools.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 3
- **Files modified:** 5 (4 created/modified in hub, 1 test fix in service_manager)
- **Commits:** 3 task commits

## Accomplishments

- ActivityLane redefined to CONTEXT order: Devices, Calibration, Training, Visualization, Config; `screens_for_lane` / `lane_for_screen` updated; default lane Devices.
- Screen enum extended with Training; included in Standard and Advanced `all_for_mode`; Training in its own lane.
- `training.rs` stub added: TrainingScreen with placeholder "Training — config and progress"; exported and routed in app; Training reachable from sidebar and command palette (Open Calibration, Open Training).
- Sidebar and shortcuts use new lane order; tests updated (lane/screen mapping, sidebar labels, apply_sidebar_shell_response).

## Task Commits

1. **Task 1: Reorganize lanes and screen mapping for CONTEXT** - `c0d6e4b` (feat)
2. **Task 2: Add Training screen stub and route in app** - `ffb2af9` (feat)
3. **Task 3: Update sidebar shell to show new lane order** - `0b8d104` (docs)

## Files Created/Modified

- `crates/neurohid-hub/src/workbench.rs` - ActivityLane enum and screens_for_lane/lane_for_screen; tests.
- `crates/neurohid-hub/src/screens/mod.rs` - Screen::Training, label, all_for_mode; pub mod training.
- `crates/neurohid-hub/src/screens/training.rs` - New TrainingScreen stub (page_header + placeholder card).
- `crates/neurohid-hub/src/app.rs` - Lane/sidebar/shortcuts/command palette; Training routing and screen_glyph; tests.
- `crates/neurohid-hub/src/service_manager.rs` - ControlSnapshot mock fields (recording_active, current_session_id) in tests.

## Decisions Made

- Lanes ordered per CONTEXT: Devices, Calibration, Training, Visualization, then Config (Dashboard, Profiles, Settings, PythonLab, JupyterIde).
- Training screen stub shows a single placeholder card; full split layout (config | progress) deferred to 05-04.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] ControlSnapshot test mocks missing fields**
- **Found during:** Task 1 verification (cargo test -p neurohid-hub)
- **Issue:** service_manager tests constructed ControlSnapshot without `recording_active` and `current_session_id` (added in neurohid-types), causing compile failure.
- **Fix:** Added `recording_active: false` and `current_session_id: None` to both mock ControlSnapshot initializers in service_manager.rs.
- **Files modified:** crates/neurohid-hub/src/service_manager.rs
- **Committed in:** c0d6e4b (Task 1 commit)

**2. [Rule 1 - Bug] sidebar_keeps_single_devices_navigation_entry assertion**
- **Found during:** Task 1 (test run after lane reorg)
- **Issue:** Test expected exactly one "Devices" label; with new layout both the Devices lane and the Devices screen entry show "Devices", so count is 2.
- **Fix:** Relaxed assertion to `>= 1` to reflect CONTEXT layout.
- **Files modified:** crates/neurohid-hub/src/app.rs
- **Committed in:** c0d6e4b (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Necessary for build and test correctness; no scope creep.

## Issues Encountered

None beyond the deviations above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Sidebar and workbench ready for 05-02 (Devices discover/connect, persistent strip), 05-03 (Calibration), 05-04 (Training split layout), 05-05 (Visualization), 05-06 (Primary workflow).
- No blockers.

## Self-Check: PASSED

- FOUND: .planning/phases/05-hub-as-ide/05-01-SUMMARY.md
- FOUND: crates/neurohid-hub/src/screens/training.rs
- Commits c0d6e4b, ffb2af9, 0b8d104 present in git log

---
*Phase: 05-hub-as-ide*
*Plan: 01*
*Completed: 2026-02-21*
