---
status: complete
phase: 05-hub-as-ide
source: 05-01-SUMMARY.md, 05-02-SUMMARY.md, 05-03-SUMMARY.md, 05-04-SUMMARY.md, 05-05-SUMMARY.md, 05-06-SUMMARY.md
started: "2026-02-21T00:00:00Z"
updated: "2026-02-21T00:00:00Z"
---

## Current Test

[testing complete]

## Tests

### 1. Sidebar lanes and Training entry
expected: Sidebar shows five lanes (Devices, Calibration, Training, Visualization, Config). Clicking Training opens the Training screen.
result: issue
reported: "Duplicate widget ID errors in sidebar: DV Devices (2EFD), CL Calibration (3552), TR Training (086D), VZ Visualization (7FBF) — each lane appears twice (Lanes + Platform) with same ID"
severity: major

### 2. Devices screen — one place for discovery and connect
expected: From Devices lane, user sees a single place for device discovery, Rescan/Connect/Disconnect, connection status, and stream health (list and health indicators).
result: issue
reported: "Disconnecting from a device doesn't update connection status/stream health/other indicators"
severity: major

### 3. Persistent status strip
expected: From any screen, the status bar shows "Devices X/Y" and "Signal %" (e.g. 0/0 and 0% when service stopped); strip is always visible.
result: pass

### 4. Calibration game list
expected: Calibration screen shows a game list/grid with Grid Maze and Target Tracking (descriptions and/or "Accuracy: —"), plus "Start full calibration" option.
result: pass
reason: (user note: requires being connected to a device first)

### 5. Calibration single-game flow
expected: Picking a game (e.g. Grid Maze) runs wizard steps then that game's panel; on completion, results are tied to active profile (user can complete or cancel and see flow).
result: issue
reported: "Can't exit mid-game; calibration screen flickers with some message that can't be seen/read"
severity: major

### 6. Training config and live panes
expected: Training screen has a config pane (model path, profile, decoder params, "Train on collected data" control) and a live progress pane showing trainer step, losses, entropy, etc. when service is running.
result: pass

### 7. Visualization open and customizable
expected: Visualization is open from the sidebar anytime; user can add/remove/arrange panels; layout is persisted (e.g. reopen Hub and same layout restored).
result: issue
reported: "Sidebar open anytime PASS. Add panels FAIL. Remove/arrange panels PASS. Layout persisted PASS. Many other issues (erroneous buttons, detach-to-window not working) deferred to own milestone/phase."
severity: major

### 8. Run and Visualization tabs
expected: When on Dashboard or Visualization in Advanced mode, a "Run | Visualization" tab bar appears above the content; switching tabs changes the view without using the sidebar.
result: pass
reason: (user requested removal: tabs feel redundant and add clutter; defer to follow-up)

### 9. Resume state
expected: After opening a screen (e.g. Calibration), closing and reopening the Hub restores the last open screen.
result: pass

### 10. Run in Hub vs Run in background in UI
expected: Settings and/or Dashboard show "Run in Hub" and "Run in background" as explicit labels (dropdown or chips).
result: pass
reason: (appears in Settings under Runtime orchestration → Service)

### 11. Suggested path on Dashboard
expected: Dashboard shows a "Suggested path" or workflow hint (e.g. Devices → Calibration → Training → Run), optionally collapsible.
result: pass

## Summary

total: 11
passed: 9
issues: 4
pending: 0
skipped: 0

## Gaps

- truth: "Sidebar shows five lanes (Devices, Calibration, Training, Visualization, Config). Clicking Training opens the Training screen."
  status: failed
  reason: "User reported: Duplicate widget ID errors in sidebar — DV Devices (2EFD), CL Calibration (3552), TR Training (086D), VZ Visualization (7FBF); each lane appears in Lanes and Platform with same widget ID"
  severity: major
  test: 1
  root_cause: "armas Sidebar generates egui widget IDs from item label; Lanes and Platform sections both listed the same labels (Devices, Calibration, etc.), causing duplicate Id. Fixed by showing a single list (Lanes only) so each label appears once."
  artifacts:
    - path: crates/neurohid-hub/src/app.rs
      issue: "render_platform_sidebar added duplicate Platform section with same labels; removed Platform items, kept Lanes only"
  missing: []

- truth: "From Devices lane, user sees a single place for device discovery, Rescan/Connect/Disconnect, connection status, and stream health (list and health indicators); indicators update after disconnect."
  status: failed
  reason: "User reported: Disconnecting from a device doesn't update connection status/stream health/other indicators"
  severity: major
  test: 2
  root_cause: "After sending Disconnect (or Connect), the Hub did not request a repaint; the next snapshot poll only runs on the next frame, and without request_repaint() the UI could idle until the user moved the mouse, so indicators appeared to never update."
  artifacts:
    - path: crates/neurohid-hub/src/screens/devices.rs
      issue: "Added ui.ctx().request_repaint() after disconnect_stream, connect_stream, disconnect_streams, connect_streams so the next frame re-polls snapshot and UI updates."
  missing: []

- truth: "Picking a game runs wizard then that game's panel; user can complete or cancel; no flickering unreadable message."
  status: failed
  reason: "User reported: Can't exit mid-game; calibration screen flickers with some message that can't be seen/read"
  severity: major
  test: 5
  root_cause: "No exit control during game: Cancel was only in wizard; mid-game (Grid Maze / Target Tracking) had no way to exit. Flickering message: not yet diagnosed (need reproducible steps or message text)."
  artifacts:
    - path: crates/neurohid-calibration/src/panel.rs
      issue: "Added 'Exit calibration' top bar and cancel_requested flow so user can exit mid-game; flickering message left for follow-up diagnosis"
  missing:
    - "If flicker persists: capture message text or repro steps (e.g. which game, when it appears) to trace source (tooltip, status_message, or hub overlay)"

- truth: "Training live progress pane shows trainer step, losses, entropy etc. without flicker; display is readable."
  status: failed
  reason: "User reported: Live progress & metrics flickers between waiting for trainer connection and metrics; some unreadable/indistinguishable display"
  severity: minor
  test: 6
  root_cause: "trainer_snapshot is polled every 1s so most frames it is None; has_metrics from snap can also alternate, so the pane flipped between 'Waiting for...' and metrics. Labels and values had low contrast."
  artifacts:
    - path: crates/neurohid-hub/src/screens/training.rs
      issue: "Sticky layout; cache last_trainer_status from poll so trainer chip doesn't flicker to 'checking…' every frame; when no metrics show single 'No metrics yet' instead of two '—' rows; reset when service stops"
  missing: []
  resolved: true

- truth: "Visualization: user can add panels (and remove/arrange); layout persisted. Deferred: erroneous buttons, detach-to-window, other Viz UX."
  status: failed
  reason: "User reported: Add panels FAIL; remove/arrange/layout PASS. Other issues (buttons, detach window) deferred to own milestone."
  severity: major
  test: 7
  artifacts: []
  missing: []
  deferred: "Visualization UX milestone: fix add panels; erroneous/unnecessary buttons; detach visualization to own window; other Viz issues"

- truth: "Run | Visualization tabs: user prefers removal (redundant, clutter)."
  status: deferred
  reason: "User passed Test 8 but requested removing Run | Visualization tabs; defer to follow-up."
  test: 8
  deferred: "Remove Run | Visualization content-area tabs from Dashboard/Visualization in Advanced mode"
