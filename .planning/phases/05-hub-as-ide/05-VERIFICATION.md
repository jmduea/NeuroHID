---
phase: 05-hub-as-ide
verified: "2026-02-21T00:00:00Z"
status: passed
score: 22/22 must-haves verified
---

# Phase 5: Hub-as-IDE Verification Report

**Phase Goal:** Hub is the IDE-like place for device setup, calibration, training, visualization, and one primary workflow.

**Verified:** 2026-02-21
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Sidebar shows Devices, Calibration, Training, Visualization (plus Config) | ✓ VERIFIED | `workbench.rs`: ActivityLane::ALL order; screens_for_lane maps Training to ActivityLane::Training; app sidebar iterates lanes CONTEXT order |
| 2 | Training is a selectable screen from the sidebar | ✓ VERIFIED | Screen::Training in mod.rs; app match arm renders TrainingScreen; lane_for_screen(Screen::Training) → ActivityLane::Training |
| 3 | Navigation is free (user can open any screen anytime) | ✓ VERIFIED | visible_screens(), set_lane(), current_screen routing; no enforced sequence |
| 4 | User can discover and connect devices from the Hub in one place | ✓ VERIFIED | devices.rs: discovered_streams, Rescan button → rescan_streams(), Connect/Disconnect → connect_stream()/disconnect_stream() via ServiceManager |
| 5 | User sees connection status and stream health in one place (Devices + strip) | ✓ VERIFIED | Devices screen shows streams, signal_quality, channel_quality; app.rs show_status_bar() shows "Devices X/Y", "Signal %" from any screen |
| 6 | User sees a list/grid of calibration games and picks one to run | ✓ VERIFIED | calibration.rs: GameKind::all() (Grid Maze, Target Tracking), StartChoice::SingleGame(kind), "Start full calibration"; CalibrationPanel::new_for_game(kind) |
| 7 | User runs calibration with wizard-style steps before each game | ✓ VERIFIED | neurohid-calibration panel has Screen::Wizard; panel flow runs wizard then game; Hub starts panel per choice |
| 8 | Calibration results are tied to the current profile for reproducibility | ✓ VERIFIED | persist_calibration_outputs() uses state.active_profile_id, profile_store; called on CalibrationPanelResult::Completed |
| 9 | User can configure decoder training (model path, params, profile/dataset) from the Hub | ✓ VERIFIED | training.rs: config pane shows model path, decoder params, active profile; "Train on collected data" trigger present (stub per plan) |
| 10 | User can launch decoder training from the Hub | ✓ VERIFIED | Trigger button present; plan allowed stub—"control protocol does not yet expose StartTraining"; training can be started via calibration or Dashboard |
| 11 | User can observe training progress and metrics in the Hub (live) | ✓ VERIFIED | Live pane uses snap.trainer_step, trainer_policy_loss, trainer_value_loss, trainer_last_error; maybe_poll_trainer_snapshot() → service_manager.trainer_snapshot() |
| 12 | User can open Visualization anytime and see real-time signal and pipeline state | ✓ VERIFIED | VisualizationScreen from sidebar; LayoutManager + DataBus + snapshot; widgets use bus/snapshot |
| 13 | User can customize visualization (add/remove/arrange panels) | ✓ VERIFIED | layout.rs LayoutManager, egui_dock; visualization_layout_preset, visualization_pane_widgets in UiConfig; take_persisted_state/from_ui_config |
| 14 | During Run, user can have Run and Visualization open together | ✓ VERIFIED | app.rs: Advanced mode shows "Run" / "Visualization" selectable labels when current_screen is Dashboard or Visualization; switch without leaving content area |
| 15 | User can follow one primary workflow without switching tools | ✓ VERIFIED | Sidebar has Devices, Calibration, Training, Visualization, Config (Dashboard/Run); docs/user-guide.md "Hub workflow: standard path" documents device → calibration → train → run |
| 16 | Run choice (Run in Hub vs Run in background) is available from the Hub | ✓ VERIFIED | ServiceRuntimeMode::Embedded/External in config; Settings UI runtime mode selector; Dashboard "Start (Run in Hub)" vs "Connect to background service" |
| 17 | When reopening the Hub, user picks up where they left off (resume state) | ✓ VERIFIED | UiConfig.last_screen; app startup applies state.config.ui.last_screen → current_screen; persist_last_screen() on screen change |

**Score:** 17/17 truths verified (all plan must-have truths covered)

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/neurohid-hub/src/workbench.rs` | Lane/screen mapping (screens_for_lane, Training) | ✓ VERIFIED | ActivityLane::Devices/Calibration/Training/Visualization/Config; screens_for_lane returns Training for lane; 259+ lines |
| `crates/neurohid-hub/src/screens/mod.rs` | Screen enum including Training | ✓ VERIFIED | Screen::Training; id "training"; all_for_mode includes Training |
| `crates/neurohid-hub/src/screens/training.rs` | Training screen stub → split layout | ✓ VERIFIED | 242 lines; split config pane + live progress pane; min_lines 80 met |
| `crates/neurohid-hub/src/app.rs` | Status bar, current_screen routing, resume | ✓ VERIFIED | show_status_bar (Devices X/Y, Signal %); current_screen routing to Training; last_screen load and persist_last_screen |
| `crates/neurohid-hub/src/screens/devices.rs` | Discover, connect, disconnect, stream health | ✓ VERIFIED | discovered_streams, rescan_streams(), connect_stream(), disconnect_stream(); stream cards and health chips |
| `crates/neurohid-hub/src/screens/calibration.rs` | Game list, persist to profile | ✓ VERIFIED | Game list (GameKind), CalibrationPanel/Result, persist_calibration_outputs, active_profile_id |
| `crates/neurohid-hub/src/service_manager.rs` | trainer_snapshot, Rescan/Connect/Disconnect | ✓ VERIFIED | trainer_snapshot(); rescan_streams, connect_stream, disconnect_stream (embedded + external) |
| `crates/neurohid-hub/src/screens/visualization.rs` | LayoutManager, DataBus, customizable panels | ✓ VERIFIED | LayoutManager, DataBus, snapshot; layout preset/pane_widgets persisted |
| `crates/neurohid-hub/src/layout.rs` | Layout preset and pane widget persistence | ✓ VERIFIED | from_ui_config(visualization_layout_preset, visualization_pane_widgets), take_persisted_state |
| `crates/neurohid-types/src/config.rs` | UiConfig last_screen for resume | ✓ VERIFIED | pub last_screen: Option<String>; serde default; doc "Last open screen ID for resume" |
| `docs/user-guide.md` | Hub workflow section | ✓ VERIFIED | "Hub workflow: standard path in the Hub"; steps 1–4 (Devices, Calibration, Training, Run); Run in Hub vs Run in background; resume state |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| app.rs | workbench/screens | sidebar rendering, current_screen routing | ✓ WIRED | visible_screens(), set_lane(), match current_screen → Training/Dashboard/Visualization/Devices/Calibration |
| devices.rs | ServiceManager / ControlSnapshot | snapshot.discovered_streams, rescan/connect/disconnect | ✓ WIRED | service_manager.rescan_streams(), connect_stream(), disconnect_stream(); snap from state.service_snapshot |
| calibration.rs | CalibrationPanel / CalibrationPanelResult | panel per selected game | ✓ WIRED | CalibrationPanel::new_for_game(kind), panel.show(), handle CalibrationPanelResult::Completed → persist_calibration_outputs |
| training.rs | ServiceSnapshot / TrainerSnapshot | state.service_snapshot, service_manager.trainer_snapshot() | ✓ WIRED | maybe_poll_trainer_snapshot() → trainer_snapshot(); snap.trainer_step, trainer_policy_loss, trainer_value_loss, trainer_last_error in UI |
| visualization.rs | state.service_snapshot / DataBus | real-time signal and pipeline state | ✓ WIRED | WidgetContext { bus, snapshot }; layout.show(ui, &ctx); widgets use bus and snapshot |
| app.rs | state.config.ui | load config and apply last_screen on init | ✓ WIRED | Hub::new: current_screen from state.config.ui.last_screen.as_deref().and_then(Screen::from_id); persist_last_screen() on screen change |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| HUB-01 | 05-02 | Discover and connect devices; connection status and stream health in one place | ✓ SATISFIED | Devices screen + status bar; discovered_streams; rescan/connect/disconnect |
| HUB-02 | 05-03 | Run calibration (wizard/games) from Hub; results tied to profile/identity | ✓ SATISFIED | Game list (Grid Maze, Target Tracking, full); panel wizard; persist_calibration_outputs(active_profile_id) |
| HUB-03 | 05-04 | Configure and launch decoder training; observe training progress and metrics | ✓ SATISFIED | Training split layout (config + live); trainer_snapshot and ControlSnapshot trainer_* in live pane; launch trigger present (stub allowed by plan) |
| HUB-04 | 05-05 | Visualize real-time signal and pipeline state; customizable; Run and Visualization together | ✓ SATISFIED | VisualizationScreen, LayoutManager, DataBus; layout persisted; Run/Visualization tabs in Advanced when on Dashboard or Visualization |
| HUB-05 | 05-01, 05-06 | One primary workflow without switching tools; Run in Hub/background; resume state | ✓ SATISFIED | Sidebar lanes; user-guide workflow; ServiceRuntimeMode in Settings; last_screen load/persist |

All phase requirement IDs (HUB-01–HUB-05) are claimed by plans and satisfied in code. No orphaned requirements.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|--------|----------|--------|
| training.rs | 108–112 | "Train on collected data" trigger no-op (no ControlCommand::StartTraining yet) | ℹ️ Info | Plan and RESEARCH explicitly allow stub; training launch via calibration/Dashboard remains available |

No blocker or warning anti-patterns. Training screen trigger is a documented follow-up.

### Human Verification Required

None required for goal achievement. Optional human checks:

1. **Run + Visualization together** — Start runtime, switch to Visualization via Run/Viz tabs (Advanced mode); confirm live data and that both views are usable.
2. **Resume state** — Close Hub on Training (or any) screen, reopen; confirm last screen is restored.
3. **Calibration → profile** — Run a calibration game to completion with an active profile; confirm profile metadata/calibration state updated (e.g. in Profiles or session).

### Gaps Summary

None. All must-haves from plans 05-01 through 05-06 are present, substantive, and wired. The only noted limitation is the Training screen "Train on collected data" button not yet sending a start-training command; this was explicitly allowed as a stub in 05-04-PLAN and does not block the phase goal (configure and observe training; launch remains possible from calibration and Dashboard).

---

_Verified: 2026-02-21_  
_Verifier: Claude (gsd-verifier)_
