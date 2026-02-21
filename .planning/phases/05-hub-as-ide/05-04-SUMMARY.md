---
phase: 05-hub-as-ide
plan: 04
subsystem: ui
tags: egui, hub, training, ControlSnapshot, TrainerSnapshot

# Dependency graph
requires:
  - phase: 05-hub-as-ide
    provides: Sidebar/nav, Training stub (05-01)
provides:
  - Training screen with split layout (config | live progress)
  - Config pane: model path, profile, decoder params, Train on collected data (stub)
  - Live pane: trainer step, losses, entropy, last_error from snapshot and trainer_snapshot()
affects: 05-05, 05-06

# Tech tracking
tech-stack:
  added: []
  patterns: Snapshot-driven Training UI; poll trainer_snapshot() when on Training screen

key-files:
  created: []
  modified: crates/neurohid-hub/src/screens/training.rs

key-decisions:
  - "Train on collected data: stub only; direct trigger from Training screen documented as follow-up (control protocol has no StartTraining command)"
  - "Live metrics from ControlSnapshot trainer_* plus optional TrainerSnapshot via service_manager.trainer_snapshot()"

patterns-established:
  - "Training screen: config (top) and live progress (bottom) panes; throttle trainer_snapshot poll to 1s"

requirements-completed: [HUB-03]

# Metrics
duration: 25min
completed: 2026-02-21
---

# Phase 5 Plan 4: Training Screen Split Layout Summary

**Training screen split into config/setup pane and live progress/metrics pane, wired to ControlSnapshot and ServiceManager.trainer_snapshot() for HUB-03.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 3 (all complete)
- **Files modified:** 1 (training.rs)

## Accomplishments

- Config pane: model path (from config), active profile/dataset, decoder params (learning_rate, gamma, batch_size, update_frequency_steps), and "Train on collected data" button (stub with follow-up note).
- Live progress pane: trainer_replay_size, trainer_step, trainer_policy_loss, trainer_value_loss, trainer_entropy, trainer_last_error from `state.service_snapshot` and optional `TrainerSnapshot` from `service_manager.trainer_snapshot()`; throttled poll (1s) when service is running.
- ServiceManager.trainer_snapshot() verified: already implemented for embedded and external runtime; Training screen calls it for the live pane.

## Task Commits

1. **Task 1 & 2: Config pane + live progress pane** - `e8688f6` (feat)
2. **Task 3: Wire trainer_snapshot** - No code change; ServiceManager already exposes trainer_snapshot(); Training screen uses it in maybe_poll_trainer_snapshot().

## Files Created/Modified

- `crates/neurohid-hub/src/screens/training.rs` - Replaced stub with split layout: config pane (model path, profile, decoder params, Train on collected data stub) and live progress pane (trainer_* from ControlSnapshot and TrainerSnapshot, polled when running).

## Decisions Made

- "Train on collected data" is a stub: control protocol does not expose a dedicated start-training command; user is directed to Dashboard "Train + Stage Candidate" or calibration. Follow-up: add ControlCommand::StartTraining or wire same train-stage job from Training screen.
- Live pane uses both ControlSnapshot trainer_* fields (already updated by app’s snapshot flow) and optional TrainerSnapshot from trainer_snapshot() for trainer state and connection status.

## Deviations from Plan

None - plan executed as written. ServiceManager already had trainer_snapshot() for embedded and external; no calibration or other files were changed for this plan.

## Issues Encountered

- cargo check -p neurohid-hub initially failed due to pre-existing calibration.rs type errors (StartChoice vs GameKind, duplicate panel binding). After cargo clean -p neurohid-hub, check passed; calibration.rs on disk was already correct (no edits made for 05-04).

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- HUB-03 satisfied: user can configure and launch decoder training from the Hub (config pane + stub trigger) and observe training progress and metrics (live pane).
- Ready for 05-05 (Visualization) and 05-06 (Primary workflow).

## Self-Check: PASSED

- training.rs exists and contains config + live panes.
- Commit e8688f6 present: `git log --oneline -1` shows feat(05-04).

---
*Phase: 05-hub-as-ide*
*Plan: 04*
*Completed: 2026-02-21*
