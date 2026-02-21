# Requirements: NeuroHID

**Defined:** 2026-02-20
**Core Value:** A single, composable path from biosignal device to actionable output — with an IDE-like experience for building and training decoders and a standalone runtime for using them — so that developers and power users don't have to piece together disparate libraries and tools.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Hub (IDE-like experience)

- [x] **HUB-01**: User can discover and connect devices from the Hub and see connection status and stream health in one place
- [ ] **HUB-02**: User can run calibration (wizard/games) from the Hub and have results tied to a profile/identity for reproducibility
- [x] **HUB-03**: User can configure and launch decoder training from the Hub and observe training progress and metrics
- [ ] **HUB-04**: User can visualize real-time signal and pipeline state (e.g. features, decoder output) in the Hub during experiments
- [x] **HUB-05**: User can follow one primary workflow in the Hub: device setup → calibration → train decoder → run (embedded or external runtime) without switching tools

### Standalone runtime

- [x] **RUNT-01**: User can start the standalone runtime (neurohid-service or equivalent) with a chosen profile and attached decoder and have it run without the Hub GUI
- [x] **RUNT-02**: User can enable/disable action output (e.g. HID) via control (CLI or Hub) while the runtime is running
- [x] **RUNT-03**: User can get runtime status (device connected, decoder loaded, output enabled, integrity) via control without opening the Hub

### Standard path

- [x] **PATH-01**: User can follow documented steps from "device in hand" to "decoder driving actions" using defaults and one coherent path
- [x] **PATH-02**: User can record/export sessions and replay or analyze them in standard formats (e.g. XDF, documented config) for reproducibility
- [x] **PATH-03**: Calibration and profile metadata are stored with version/identity so the same setup can be reproduced

### Composable SDK / CLI / formats

- [x] **COMP-01**: Developer can drive device discovery, connection, and stream selection via public SDK API (Rust) and/or CLI
- [x] **COMP-02**: Developer can configure signal pipeline and decoder (e.g. model path, params) via SDK/CLI and documented config format
- [x] **COMP-03**: Developer can start/stop runtime and send control requests (e.g. snapshot, set output enabled) via SDK or CLI
- [x] **COMP-04**: Stream consumption, timestamp, and "latest sample" semantics are documented and consistent (e.g. drain-then-last for LSL)
- [x] **COMP-05**: Profile and config formats are versioned and have a documented compatibility policy
- [ ] **COMP-06**: User can replace any pipeline component (acquisition, signal preprocessing, decoder, or output) with a custom or third-party implementation that conforms to the published contract, and the rest of the pipeline plus control, Hub, runtime, and observability still integrate with it (no loss of ecosystem benefits)

### Extensibility

- [ ] **EXT-01**: New device backends can be added without changing core orchestration (trait-based or plugin contract)
- [ ] **EXT-02**: New action/output types (e.g. game input, MIDI) can be added via a defined outlet/effector contract
- [ ] **EXT-03**: One example plugin (device or outlet) exists and is tested in CI to demonstrate the extension path

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Hub

- **HUB-06**: Advanced visualization and experiment templates in the Hub
- **HUB-07**: Richer Python/notebook integration and workbench layout options

### Runtime and performance

- **RUNT-04**: Latency profiling, tuning, and optional latency guarantees (p95/p99) where feasible
- **RUNT-05**: Soak and LatencyMatrix criteria documented and enforced in CI

### Extensibility

- **EXT-04**: Plugin discovery/lifecycle (dynamic lib, subprocess, or in-process) fully specified and documented

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Mandatory cloud or hosted backend | Local-first; no required cloud auth or sync |
| Mobile app as v1 surface | Desktop and headless first per PROJECT.md |
| General end-consumer product before power-user tooling | Target is developers and power users first |
| GUI-only for critical operations | CLI/SDK must support automation and headless use |
| Opaque or proprietary data/model formats | Use open, documented formats (ONNX, XDF, versioned config) |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| HUB-01 | Phase 5 | Complete |
| HUB-02 | Phase 5 | Pending |
| HUB-03 | Phase 5 | Complete |
| HUB-04 | Phase 5 | Pending |
| HUB-05 | Phase 5 | Complete |
| RUNT-01 | Phase 2 | Complete |
| RUNT-02 | Phase 2 | Complete |
| RUNT-03 | Phase 2 | Complete |
| PATH-01 | Phase 4 | Complete |
| PATH-02 | Phase 4 | Complete |
| PATH-03 | Phase 1 | Complete |
| COMP-01 | Phase 3 | Complete |
| COMP-02 | Phase 3 | Complete |
| COMP-03 | Phase 2 | Complete |
| COMP-04 | Phase 1 | Complete |
| COMP-05 | Phase 1 | Complete |
| COMP-06 | Phase 6 | Pending |
| EXT-01 | Phase 6 | Pending |
| EXT-02 | Phase 6 | Pending |
| EXT-03 | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 19 total
- Mapped to phases: 19
- Unmapped: 0 ✓

---
*Requirements defined: 2026-02-20*
*Last updated: 2026-02-20 after initial definition*
