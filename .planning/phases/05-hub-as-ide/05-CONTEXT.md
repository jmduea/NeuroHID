# Phase 5: Hub-as-IDE - Context

**Gathered:** 2026-02-21
**Status:** Ready for planning

## Phase Boundary

Hub is the IDE-like place where users discover/connect devices, see connection and stream health, run calibration games tied to a profile, configure and launch decoder training with progress and metrics, and visualize real-time signal and pipeline state. Users follow one primary workflow (device setup → calibration → train decoder → run) without switching tools. Run can be embedded in the Hub or external (e.g. neurohid-service). This phase delivers that experience; advanced viz templates and richer Python/notebook integration are later phases.

## Implementation Decisions

### Layout and navigation

- Sidebar always visible, VSCode-like: sidebar + content area (not top tabs, not dashboard-first).
- Device/stream status: both a persistent strip (at-a-glance from anywhere) and a dedicated Devices screen for detail.
- Three top-level sidebar items:
  - **Calibration** — entrypoint for games the user runs to calibrate device/decoder.
  - **Training** — everything related to decoder training: configuration, stats, observability.
  - **Visualization** — customizable, comprehensive view of all observable streams and metrics.
- Settings, Profiles, Python/notebook entry: placement and weight left to implementer (Claude’s discretion).

### Primary workflow shape

- Free navigation: user can open Devices / Calibration / Training / Run anytime; suggested order in docs or UI, not enforced.
- Run: user choice — “Run in Hub” (embedded) or “Run in background” (external); both available from the Hub.
- Training occurs in the background / during calibration; not a separate step after calibration. Progress/next-step UI should reflect that.
- Resume: when opening the Hub, pick up wherever they left off (resume state / “where you left off”).

### Calibration experience

- Calibration top-level shows a list or grid of calibration games; user picks one to run.
- Show metrics for how accurate the trained profile’s decoder is for that specific game when applicable.
- One “current profile” (e.g. from Devices or global); calibration games always write to that profile.
- Wizard-style: mandatory steps/explanations before each game or phase.
- Training visibility: light “training in progress” in Calibration; full training stats and detail in the Training view.

### Training and visualization placement

- Training view: split layout — config/setup (e.g. model path, params, dataset/profile) on one side, live progress and metrics on the other when training runs.
- Training trigger: calibration auto-starts training. Training view can also trigger training on existing profile data (e.g. retrain, train on collected data).
- Visualization: separate view — user can open it anytime; during Run they can have Run and Visualization open together (e.g. side by side or tabs).
- Visualization is customizable: user can add/remove/arrange panels (which streams, which metrics, layout).

### Claude's Discretion

- Where to place Settings, Profiles, and Python/notebook entry in the sidebar or nav (secondary/menu vs equal weight).
- Exact wizard copy and step breakdown for calibration.
- Preset vs custom default for Visualization layout.
- Concrete “resume / where you left off” behavior (e.g. last open view, or first incomplete step).

## Specific Ideas

- “Think VSCode” for sidebar + content layout.
- Calibration as the entrypoint for games; Training = decoder training config/stats/observability; Visualization = comprehensive, customizable view of streams/metrics.
- Decoder accuracy metrics per game (when applicable) on the calibration game list/grid.
- Pick up wherever they left off when reopening the Hub.

## Deferred Ideas

- None — discussion stayed within phase scope. (HUB-06 advanced visualization / experiment templates and HUB-07 Python/notebook workbench are already in roadmap/requirements as later.)

---

*Phase: 05-hub-as-ide*
*Context gathered: 2026-02-21*
