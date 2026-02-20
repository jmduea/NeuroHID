# Roadmap: NeuroHID

## Overview

This roadmap takes NeuroHID from current validated foundations (device/signal/decoder/action stack, IPC, Hub GUI, headless runtime, storage, calibration, SDK) to v1 release: versioned contracts and formats first, then standalone runtime control, SDK/CLI for device and config, a documented standard path with recording, Hub-as-IDE, and finally composable replacement and extensibility so the pipeline is swappable and new device/output types can be added.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Contracts and versioned formats** - Profile/config and stream semantics versioned and documented; calibration/profile metadata reproducible (2026-02-20)
- [x] **Phase 2: Standalone runtime and control** - Run headless with profile+decoder; enable/disable output and get status via control; SDK/CLI for runtime control (completed 2026-02-20)
- [x] **Phase 3: SDK/CLI for device and pipeline config** - Developer drives discovery, connection, stream selection, and configures signal/decoder via SDK/CLI (completed 2026-02-20)
- [ ] **Phase 4: Standard path and recording** - One documented path device→decoder→actions; record/export/replay in standard formats
- [ ] **Phase 5: Hub-as-IDE** - Discover/connect, calibrate, train, visualize, and one primary workflow in the Hub
- [ ] **Phase 6: Composable and extensible** - Replace any pipeline component via contract; add device/output types; one example plugin in CI

## Phase Details

### Phase 1: Contracts and versioned formats
**Goal**: Config, profile, and stream semantics are versioned and documented so the same setup can be reproduced.
**Depends on**: Nothing (first phase)
**Requirements**: PATH-03, COMP-04, COMP-05
**Success Criteria** (what must be TRUE):
  1. Profile and config formats have a documented version and compatibility policy
  2. Stream consumption, timestamp, and "latest sample" semantics are documented (e.g. drain-then-last for LSL)
  3. Calibration and profile metadata are stored with version/identity so the same setup can be reproduced
**Plans**: 3 plans

Plans:
- [x] 01-01-PLAN.md — Config format versioned and documented (COMP-05)
- [x] 01-02-PLAN.md — Profile format versioned; calibration/profile identity for reproducibility (COMP-05, PATH-03)
- [x] 01-03-PLAN.md — Stream semantics documented (consumption, timestamps, latest-sample) (COMP-04)

### Phase 2: Standalone runtime and control
**Goal**: User can run the decoder in the background and control it without the Hub GUI.
**Depends on**: Phase 1
**Requirements**: RUNT-01, RUNT-02, RUNT-03, COMP-03
**Success Criteria** (what must be TRUE):
  1. User can start the standalone runtime (e.g. neurohid-service) with a chosen profile and attached decoder and have it run without the Hub GUI
  2. User can enable or disable action output (e.g. HID) via control (CLI or Hub) while the runtime is running
  3. User can get runtime status (device connected, decoder loaded, output enabled, integrity) via control without opening the Hub
  4. Developer can start/stop runtime and send control requests (e.g. snapshot, set output enabled) via SDK or CLI
**Plans**: 2 plans

Plans:
- [x] 02-01-PLAN.md — Standalone service with default control endpoint and startup docs (RUNT-01)
- [x] 02-02-PLAN.md — Control CLI (snapshot, set-output-enabled) and docs (RUNT-02, RUNT-03, COMP-03)

### Phase 3: SDK/CLI for device and pipeline config
**Goal**: Developer can drive device discovery and configure the signal/decoder pipeline via public API and CLI.
**Depends on**: Phase 2
**Requirements**: COMP-01, COMP-02
**Success Criteria** (what must be TRUE):
  1. Developer can drive device discovery, connection, and stream selection via public SDK API (Rust) and/or CLI
  2. Developer can configure signal pipeline and decoder (e.g. model path, params) via SDK/CLI and documented config format
**Plans**: 2 plans

Plans:
- [ ] 03-01-PLAN.md — SDK device discovery/connection API and CLI device list/connect (COMP-01)
- [ ] 03-02-PLAN.md — Config YAML + docs, SDK config API, CLI config/pipeline subcommands (COMP-02)

### Phase 4: Standard path and recording
**Goal**: User has one coherent path from device to actions and can record/replay sessions for reproducibility.
**Depends on**: Phase 3
**Requirements**: PATH-01, PATH-02
**Success Criteria** (what must be TRUE):
  1. User can follow documented steps from "device in hand" to "decoder driving actions" using defaults and one coherent path
  2. User can record/export sessions and replay or analyze them in standard formats (e.g. XDF, documented config) for reproducibility
**Plans**: TBD

Plans:
- [ ] 04-01: TBD

### Phase 5: Hub-as-IDE
**Goal**: Hub is the IDE-like place for device setup, calibration, training, visualization, and one primary workflow.
**Depends on**: Phase 4
**Requirements**: HUB-01, HUB-02, HUB-03, HUB-04, HUB-05
**Success Criteria** (what must be TRUE):
  1. User can discover and connect devices from the Hub and see connection status and stream health in one place
  2. User can run calibration (wizard/games) from the Hub and have results tied to a profile/identity for reproducibility
  3. User can configure and launch decoder training from the Hub and observe training progress and metrics
  4. User can visualize real-time signal and pipeline state (e.g. features, decoder output) in the Hub during experiments
  5. User can follow one primary workflow in the Hub: device setup → calibration → train decoder → run (embedded or external runtime) without switching tools
**Plans**: TBD

Plans:
- [ ] 05-01: TBD

### Phase 6: Composable and extensible
**Goal**: Pipeline components are swappable and new device/output types can be added via published contracts.
**Depends on**: Phase 5
**Requirements**: COMP-06, EXT-01, EXT-02, EXT-03
**Success Criteria** (what must be TRUE):
  1. User/developer can replace any pipeline component (acquisition, signal preprocessing, decoder, or output) with a custom or third-party implementation that conforms to the published contract, and the rest of the pipeline plus control, Hub, runtime, and observability still integrate
  2. New device backends can be added without changing core orchestration (trait-based or plugin contract)
  3. New action/output types (e.g. game input, MIDI) can be added via a defined outlet/effector contract
  4. One example plugin (device or outlet) exists and is tested in CI to demonstrate the extension path
**Plans**: TBD

Plans:
- [ ] 06-01: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Contracts and versioned formats | 3/3 | Complete | 2026-02-20 |
| 2. Standalone runtime and control | 2/2 | Complete    | 2026-02-20 |
| 3. SDK/CLI for device and pipeline config | 2/2 | Complete   | 2026-02-20 |
| 4. Standard path and recording | 0/? | Not started | - |
| 5. Hub-as-IDE | 0/? | Not started | - |
| 6. Composable and extensible | 0/? | Not started | - |
