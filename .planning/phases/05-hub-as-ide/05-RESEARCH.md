# Phase 5: Hub-as-IDE - Research

**Researched:** 2026-02-21
**Domain:** IDE-like Hub GUI (egui/eframe), device/calibration/training/visualization workflow, Rust desktop UI patterns
**Confidence:** HIGH (in-repo code and types); MEDIUM (UX/resume behavior)

## Summary

Phase 5 delivers the Hub as the single IDE-like surface for device setup, calibration, decoder training, visualization, and one primary workflow (device → calibration → train → run) without switching tools. The codebase already provides the core stack (egui, eframe, armas, egui_dock), service manager (embedded vs external runtime), control/snapshot types (device discovery, stream health, trainer metrics), Devices and Calibration and Visualization screens, and a VSCode-style sidebar. Gaps to plan for: (1) **Sidebar alignment** with CONTEXT’s three top-level items (Calibration, Training, Visualization) plus Devices (strip + screen) and placement of Dashboard/Profiles/Settings; (2) **Training screen** — dedicated view with split layout (config/setup vs live progress and metrics), fed by existing `ControlSnapshot`/`TrainerSnapshot` and optional manual trigger; (3) **Calibration as game list** — entrypoint showing a list/grid of games (e.g. Grid Maze, Target Tracking) with optional decoder-accuracy metrics per game, then wizard + run; (4) **Persistent device/stream strip** — at-a-glance status from anywhere (status bar already shows Devices X/Y and signal %; may enhance or formalize); (5) **Resume / “where you left off”** — e.g. last open view or first incomplete step, with optional `UiConfig` persistence.

**Primary recommendation:** Implement within the existing neurohid-hub + neurohid-calibration + neurohid-types surface: add a Training screen and sidebar reorganization, introduce a calibration game picker that launches the existing panel per game, and add optional resume state in config/window init. Do not introduce a new GUI stack or reimplement device/trainer protocols.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **Layout and navigation:** Sidebar always visible, VSCode-like (sidebar + content area). Device/stream status: both a persistent strip and a dedicated Devices screen. Three top-level sidebar items: **Calibration**, **Training**, **Visualization**. Settings, Profiles, Python/notebook entry: placement and weight left to implementer (Claude’s discretion).
- **Primary workflow:** Free navigation; Run choice — “Run in Hub” (embedded) or “Run in background” (external); both from Hub. Training occurs in background / during calibration; progress/next-step UI reflects that. Resume: when opening Hub, pick up wherever they left off.
- **Calibration:** Top-level shows list or grid of calibration games; user picks one to run. Show metrics for decoder accuracy per game when applicable. One “current profile”; calibration games write to that profile. Wizard-style: mandatory steps/explanations before each game or phase. Training visibility: light “training in progress” in Calibration; full training stats in Training view.
- **Training and visualization:** Training view: split layout — config/setup on one side, live progress and metrics on the other. Calibration auto-starts training; Training view can also trigger training on existing profile data. Visualization: separate view; open anytime; during Run, Run and Visualization can be open together. Visualization is customizable (add/remove/arrange panels).

### Claude's Discretion

- Where to place Settings, Profiles, and Python/notebook entry in the sidebar or nav (secondary/menu vs equal weight).
- Exact wizard copy and step breakdown for calibration.
- Preset vs custom default for Visualization layout.
- Concrete “resume / where you left off” behavior (e.g. last open view, or first incomplete step).

### Deferred Ideas (OUT OF SCOPE)

- None — discussion stayed within phase scope. (HUB-06 advanced visualization / experiment templates and HUB-07 Python/notebook workbench are already in roadmap/requirements as later.)

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| HUB-01 | User can discover and connect devices from the Hub and see connection status and stream health in one place | `ControlSnapshot.discovered_streams`, `DiscoveredStream` (id, name, connected, channel_quality, integrity_state, etc.), `RescanStreams`/`ConnectStream`/`DisconnectStream`; Devices screen and status bar already show streams and health; enhance persistent strip if needed. |
| HUB-02 | User can run calibration (wizard/games) from the Hub and have results tied to a profile/identity for reproducibility | `neurohid-calibration` panel + wizard + games (GridMaze, TargetTracking); Hub Calibration screen wraps panel and persists to active profile; add game list/grid entrypoint and optional per-game decoder metrics. |
| HUB-03 | User can configure and launch decoder training from the Hub and observe training progress and metrics | `ControlSnapshot` trainer_* fields and `TrainerSnapshot`; `TrainerSnapshot` control command; ServiceManager `trainer_snapshot()`; Dashboard already shows trainer status; add dedicated Training screen with split (config vs live progress) and optional manual trigger. |
| HUB-04 | User can visualize real-time signal and pipeline state in the Hub during experiments | Visualization screen + `LayoutManager` + egui_dock; `DataBus` and snapshot; customizable panels; Run and Visualization open together (tabs/side-by-side). |
| HUB-05 | User can follow one primary workflow in the Hub: device setup → calibration → train decoder → run (embedded or external) without switching tools | Sidebar + free navigation; embedded vs external via `ServiceRuntimeMode`; single workflow documented/suggested in UI; resume state so returning users continue from last step/view. |

</phase_requirements>

## Standard Stack

### Core (already in use)

| Library | Version / ref | Purpose | Why Standard |
|---------|----------------|---------|--------------|
| egui | workspace | Immediate-mode GUI | Project standard; neurohid-hub and neurohid-calibration use it. |
| eframe | workspace | Window/backend (glow, platform) | Standard egui app shell. |
| armas | 0.1.2 | Components (Sidebar, buttons, theme) | Used for VSCode-like sidebar and styling in `app.rs`. |
| egui_dock | 0.18.0 | Docking/tabbed panes | Used for Visualization layout (`layout.rs`, `visualization.rs`). |

### Supporting (already in use)

| Library | Purpose | When to Use |
|---------|---------|-------------|
| neurohid-types | ControlSnapshot, DiscoveredStream, TrainerSnapshot, UiConfig, config | All Hub screens and service manager. |
| neurohid-core (facade) | Runtime, control, device API | Hub depends on core for runtime/snapshot, not device/signal directly. |
| neurohid-calibration | CalibrationPanel, games, wizard | Calibration screen embeds panel; games list can wrap same types. |
| neurohid-storage | ProfileStore, ConfigStore | Profile/config load/save and calibration persistence. |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|-----------|
| egui/eframe | iced, Tauri+web | Not in scope; stack is fixed per existing codebase. |
| armas Sidebar | Custom egui panels | armas already provides Sidebar + CollapsibleMode; keep for consistency. |

**Installation:** No new crates required for Phase 5; stack is present in `crates/neurohid-hub/Cargo.toml` and `crates/neurohid-calibration/Cargo.toml`.

## Architecture Patterns

### Recommended Project Structure (existing, extend)

```
crates/neurohid-hub/src/
├── app.rs              # Sidebar, status bar, screen routing; extend for Training + nav alignment
├── workbench.rs        # Lanes/screens; add Training to lanes and screen enum
├── state.rs            # HubState, ServiceSnapshot (ControlSnapshot)
├── service_manager.rs  # Embedded/external, snapshot, trainer_snapshot(), Rescan/Connect/Disconnect
├── screens/
│   ├── mod.rs          # Screen enum; add Training
│   ├── devices.rs      # Discover/connect, stream health (keep + optional strip enhancement)
│   ├── calibration.rs  # Add game list/grid entrypoint; keep panel wrapper
│   ├── dashboard.rs    # Start/stop, profile, trainer summary; keep
│   ├── visualization.rs # Customizable panes; keep
│   └── training.rs     # NEW: split layout config | live progress/metrics
├── layout.rs           # Visualization dock layout (keep)
└── widgets/            # Visualization widgets (keep)
```

### Pattern 1: VSCode-like sidebar + content

**What:** Left sidebar (armas `Sidebar`, `SidebarState`) + central content area; sidebar shows lanes/screens; status bar at bottom.
**Where:** `app.rs` `show_sidebar()`, `render_sidebar_shell()`, `show_status_bar()`; `workbench.rs` `visible_screens()`, `screens_for_lane()`.
**Example (existing):** `egui::SidePanel::left("sidebar")` with `render_sidebar_shell()`; `apply_sidebar_shell_response()` drives `current_screen`. CONTEXT requires three top-level items (Calibration, Training, Visualization) plus Devices; Dashboard/Profiles/Settings placement at implementer’s discretion.

### Pattern 2: Snapshot-driven UI

**What:** All device/stream/trainer state comes from `ServiceSnapshot` (alias for `ControlSnapshot`) plus optional `TrainerSnapshot`; Hub polls or receives events via ServiceManager.
**Where:** `state.service_snapshot`; `service_manager.snapshot_embedded()` / external poll; `ControlSnapshot.discovered_streams`, `trainer_*`, `signal_quality`, etc.
**Example:** Devices screen uses `snap.discovered_streams`, `snap.routed_*`, `snap.signal_quality`; status bar shows `Devices {connected}/{total}`, Signal %, etc. Training screen should use same snapshot + `trainer_snapshot()` for live panel.

### Pattern 3: Embedded vs external runtime

**What:** `ServiceRuntimeMode::Embedded` runs runtime in-process; `External` uses existing neurohid-service over IPC. Both expose same control/snapshot surface to Hub.
**Where:** `service_manager.rs`: `start_embedded()`, `start_external_event_worker()`, snapshot from embedded handle or control polling.
**Example:** Settings already show “Runtime embedded” vs external; Dashboard starts/stops service; “Run in Hub” = embedded, “Run in background” = external (user chooses mode then starts).

### Anti-Patterns to Avoid

- **Hub depending on neurohid-device/neurohid-signal directly:** Use neurohid-core facade and control/snapshot only (per crate-boundaries.md).
- **Duplicating snapshot fields in Hub state:** Use `ServiceSnapshot` (= `ControlSnapshot`) as single source of truth; no parallel “hub view” of streams.
- **New GUI framework or docking library:** Stay on egui + egui_dock + armas.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Device discovery/connection | Custom discovery UI protocol | ControlSnapshot.discovered_streams + RescanStreams/ConnectStream/DisconnectStream | Runtime already owns discovery; CLI/SDK use same commands. |
| Trainer progress / metrics | Custom training protocol | ControlSnapshot trainer_* + TrainerSnapshot command | Runtime and Python bridge already emit trainer state. |
| Docking/tabbed layout | Custom layout engine | egui_dock (existing in Visualization) | Complex layout and persistence already handled. |
| Profile/decoder persistence | Custom storage | ProfileStore, ConfigStore, calibration persistence in Calibration screen | Storage crate and existing persist_calibration_outputs. |
| Resume / last view | Ad-hoc only in memory | Optional UiConfig field (e.g. last_screen or last_lane) persisted with config | Config already persisted; one extra field avoids reimplementing persistence. |

**Key insight:** Device, training, and config contracts are already defined in neurohid-types and implemented in neurohid-core/service; Hub should only consume them and present them in the agreed layout.

## Common Pitfalls

### Pitfall 1: Sidebar vs CONTEXT mismatch

**What goes wrong:** Keeping current lane list (e.g. Ops = Dashboard, Devices, Profiles, Calibration) without a clear “Training” top-level and without aligning to “Calibration, Training, Visualization” as the three main items.
**Why it happens:** CONTEXT was written after current sidebar; workbench.rs maps screens to lanes, not the other way around.
**How to avoid:** Plan an explicit sidebar reorganization: e.g. three primary entries (Calibration, Training, Visualization), Devices as its own entry or under a “Setup” group, Dashboard/Profiles/Settings per discretion; update `Screen` enum and `screens_for_lane` / `lane_for_screen` to match.
**Warning signs:** Training only reachable from Dashboard or buried under Ops.

### Pitfall 2: Training screen without split layout

**What goes wrong:** Training view is a single pane (e.g. only metrics or only config), so HUB-03 “configure and launch” and “observe progress” are not both visible.
**Why it happens:** Easiest first implementation is one panel.
**How to avoid:** Design Training screen with two areas: one for config/setup (model path, profile, params, trigger “train on existing data”) and one for live progress (trainer_* from snapshot, steps, loss, errors).
**Warning signs:** No place to trigger training from existing data; no live metrics when calibration is not running.

### Pitfall 3: Calibration as single linear flow only

**What goes wrong:** User cannot choose “run only Grid Maze” or “run only Target Tracking”; CONTEXT asks for “list or grid of calibration games; user picks one to run.”
**Why it happens:** Current CalibrationPanel runs Welcome → Wizard → GridMaze → TargetTracking → Complete in sequence.
**How to avoid:** Add a Calibration top-level “game list” (list or grid) that shows available games (e.g. Grid Maze, Target Tracking); selecting one runs wizard (if desired) then that game’s panel; optionally show decoder accuracy per game when backend supports it.
**Warning signs:** No game selection; user always forced through both games.

### Pitfall 4: Forgetting external runtime for “Run in background”

**What goes wrong:** “Run in Hub” works but “Run in background” (external) is unclear or broken in UI.
**Why it happens:** Most testing is embedded.
**How to avoid:** Ensure Settings (or equivalent) exposes runtime mode (Embedded vs External) and that when External is selected, Hub still shows status/snapshot via control polling; document that user must start neurohid-service separately for external.
**Warning signs:** No way to switch to External; snapshot stays empty when in External and service is running.

### Pitfall 5: Resume state not persisted

**What goes wrong:** Reopening Hub always lands on same default screen; “pick up where you left off” is not implemented.
**Why it happens:** UiConfig does not currently store last screen or “first incomplete step.”
**How to avoid:** Add optional field(s) to UiConfig (e.g. `last_screen` or `last_sidebar_id`) and apply on startup; or derive “first incomplete step” from profile/calibration state and offer a shortcut. Document behavior in CONTEXT “resume” discretion.
**Warning signs:** No config key for last view; app always opens to Dashboard or first lane.

## Code Examples

Verified patterns from the existing codebase:

### Device/stream status in status bar (existing)

```rust
// app.rs show_status_bar() — Devices count and signal from snapshot
let connected_streams = snap.discovered_streams.iter().filter(|s| s.connected).count();
let total_streams = snap.discovered_streams.len();
theme::status_chip(ui, &format!("Devices {}/{}", connected_streams, total_streams), ...);
theme::status_chip(ui, &format!("Signal {:.0}%", snap.signal_quality * 100.0), ...);
```

### Rescan / Connect / Disconnect from Hub

```rust
// service_manager sends control commands; Devices screen calls:
service_manager.rescan_streams();
// Connect: send_control_request(ControlRequest::new(ControlCommand::ConnectStream { stream_id }))
// Disconnect: ControlCommand::DisconnectStream { stream_id }
```

### Trainer snapshot for Training screen

```rust
// service_manager.rs
pub fn trainer_snapshot(&mut self) -> Option<TrainerSnapshot> {
    // Embedded: rt.trainer_snapshot()
    // External: send_control_request(ControlCommand::TrainerSnapshot) -> ControlResponsePayload::TrainerSnapshot { snapshot }
}
// ControlSnapshot already has: trainer_replay_size, trainer_step, trainer_policy_loss, trainer_value_loss, trainer_entropy, trainer_last_error
```

### Calibration panel and persistence to profile

```rust
// screens/calibration.rs — panel result and persist to active profile
CalibrationPanelResult::Completed(quality) => {
    self.persist_calibration_outputs(state, runtime, &quality);
}
// persist_calibration_outputs uses state.active_profile_id, profile_store, config
```

### Visualization layout from config

```rust
// layout.rs, visualization.rs
LayoutManager::from_ui_config(ui_config)  // visualization_layout_preset, visualization_pane_widgets
self.layout.take_persisted_state() -> PersistedLayoutState
state.config.ui.visualization_layout_preset = persisted.layout_preset;
```

## State of the Art

| Area | Current in Repo | Phase 5 Target |
|------|-----------------|----------------|
| Sidebar | Lanes (Ops, Analysis, Labs, Config) with screens per lane | Three top-level: Calibration, Training, Visualization; Devices + others per CONTEXT |
| Training | Trainer status on Dashboard only | Dedicated Training screen, split config | progress |
| Calibration | Single panel flow (Welcome → Wizard → Grid → Tracking → Complete) | Game list/grid → pick game → wizard + run; optional accuracy per game |
| Device status | Status bar (Devices X/Y, Signal %) + Devices screen | Keep; optional persistent strip refinement |
| Resume | None | Optional last_screen or “first incomplete step” in UiConfig |

**Deprecated/outdated:** N/A for this phase; no removals required.

## Open Questions

1. **Decoder accuracy per game**
   - What we know: CONTEXT asks to “show metrics for how accurate the trained profile’s decoder is for that specific game when applicable.”
   - What’s unclear: Whether backend or profile already stores per-game accuracy; may be a stub or “N/A” until backend supports it.
   - Recommendation: Add a placeholder in the game list (e.g. “Accuracy: —” or “N/A”); implement real values when types/backend provide them.

2. **“First incomplete step” for resume**
   - What we know: CONTEXT allows “last open view” or “first incomplete step” as discretion.
   - What’s unclear: Definition of “incomplete” (e.g. no device connected, no calibration, no training run).
   - Recommendation: Prefer simple “last open view” in UiConfig first; optional “suggest next step” banner based on snapshot/profile state.

3. **Training “trigger on existing data”**
   - What we know: CONTEXT says “Training view can also trigger training on existing profile data (e.g. retrain, train on collected data).”
   - What’s unclear: Whether this is a control command today or a Python-side workflow only.
   - Recommendation: Check neurohid-core/control and Python trainer for “start training from replay/session”; if present, add button in Training config pane; if not, add stub and document as follow-up.

## Sources

### Primary (HIGH confidence)

- `crates/neurohid-hub/src/app.rs` — sidebar, status bar, screen routing
- `crates/neurohid-hub/src/workbench.rs` — lanes, screens_for_lane, lane_for_screen
- `crates/neurohid-hub/src/service_manager.rs` — embedded/external, snapshot, trainer_snapshot, control commands
- `crates/neurohid-types/src/control.rs` — ControlSnapshot, TrainerSnapshot, ControlCommand
- `crates/neurohid-types/src/device.rs` — DiscoveredStream
- `crates/neurohid-types/src/config.rs` — UiConfig, SystemConfig
- `crates/neurohid-calibration/src/panel.rs`, `games/mod.rs` — CalibrationPanel, games
- `docs/crate-boundaries.md` — Hub depends on core facade only
- `docs/architecture-rust-core.md` — Hub screens, egui_dock, armas

### Secondary (MEDIUM confidence)

- `docs/integration-architecture.md` — control snapshot, integrity, device discovery
- `docs/user-guide.md` — standard path, Hub role
- `.planning/phases/05-hub-as-ide/05-CONTEXT.md` — locked decisions and discretion

### Tertiary (LOW confidence)

- Decoder accuracy per game: no in-repo type found; treat as optional/placeholder until backend exists.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all libraries and patterns are in the repo and documented.
- Architecture: HIGH — crate boundaries, snapshot types, and screen layout are implemented; only Training screen and sidebar/game-list changes are net new.
- Pitfalls: MEDIUM — pitfalls are inferred from CONTEXT vs current code; “resume” and “accuracy per game” depend on product decisions.

**Research date:** 2026-02-21
**Valid until:** ~30 days; re-check if neurohid-types or control protocol gain new fields (e.g. per-game accuracy, explicit “start training” command).
