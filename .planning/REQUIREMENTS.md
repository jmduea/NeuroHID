# Requirements: NeuroHID v1.1

**Defined:** 2026-02-21  
**Core Value:** A single, composable path from biosignal device to actionable output — with an IDE-like experience for building and training decoders and a standalone runtime for using them — so that developers and power users don't have to piece together disparate libraries and tools.

## v1.1 Requirements

Requirements for milestone v1.1 (Testing, BrainFlow & Framework Separation). Each maps to roadmap phases.

### Testing

- [x] **TEST-01**: Developer gets deterministic test runs (no flakiness from async/concurrency or shared state) via test policy and tooling (e.g. nextest, condition-based waits, isolation)
- [x] **TEST-02**: Developer has integration tests at key boundaries (IPC Rust↔Python, device→signal→decoder→action pipeline, config load/save) so interface mismatches are caught in CI
- [x] **TEST-03**: CI gates reflect reality (coverage and flakiness addressed) so passing CI means safe-to-merge for the scope exercised
- [x] **TEST-04**: Developer has at least one valuable E2E path (e.g. Hub discover→connect→stream or runtime profile→decoder→action) exercised in tests
- [x] **TEST-05**: Test tiers and isolation policy are documented so contributors know what is unit vs integration vs E2E and how to avoid flakiness

### Framework–Hub separation

- [x] **FRAME-01**: Developer can identify the documented framework surface (which crates and features to depend on) and use it without depending on Hub internals
- [x] **FRAME-02**: Hub is documented as one application built on top of the framework (same binaries; dependency graph and docs define the boundary)
- [x] **FRAME-03**: Hub depends only on core (and calibration) and the framework facade; dependency audit or CI check enforces no disallowed direct deps from Hub to component crates
- [x] **FRAME-04**: Docs describe the framework vs Hub boundary (e.g. in crate-boundaries or framework-surface) so contributors and embedders know what is framework vs application

### BrainFlow — first-class (docs, examples, Hub UX)

- [ ] **BRAIN-01**: User or developer can read first-class documentation for using BrainFlow with NeuroHID (setup, config, synthetic vs native hardware, build order)
- [ ] **BRAIN-02**: User can run at least one runnable example using BrainFlow’s synthetic board that demonstrates BrainFlow with NeuroHID end-to-end
- [ ] **BRAIN-03**: User can discover and connect BrainFlow devices from the Hub Devices screen with UX parity to LSL and other backends (discover, connect, disconnect)
- [ ] **BRAIN-04**: BrainFlow remains one backend behind the existing DeviceProvider/Device abstraction; device-agnostic API is preserved
- [ ] **BRAIN-05**: BrainFlow’s synthetic board fully replaces the in-repo mock device: tests, examples, and CI use the synthetic board as the single non-hardware device path (no separate mock backend)

### BrainFlow — deeper (real SDK, streaming)

- [ ] **BRAIN-06**: Developer can build and use the real BrainFlow SDK in neurohid-device behind a feature flag (e.g. brainflow-native); default and CI use synthetic board only (no mock)
- [ ] **BRAIN-07**: User or developer can use the BrainFlow streaming path into the same signal pipeline as LSL and other backends (synthetic or real board → Device → pipeline)
- [ ] **BRAIN-08**: BrainFlow version and build steps (C++ core then Rust) are pinned and documented so builds are reproducible

## Future requirements (v1.2+)

Deferred to future milestones. Tracked but not in current roadmap.

### Testing

- **TEST-06**: Validation harness (Soak, LatencyMatrix, BootMatrix) runs in CI and fails the build on regressions (RUNT-04, RUNT-05 alignment)

### BrainFlow

- **BRAIN-09**: User can configure board-specific params (serial port, IP, timeout, presets) in the Hub for BrainFlow devices

### Framework

- **FRAME-05**: Framework is split to a separate repo or publishable package; Hub consumes it as a dependency (planned down the road)

## Out of scope

Explicitly excluded for v1.1. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| E2E for every feature | Test pyramid: many unit, some integration, few E2E for critical paths only (research) |
| BrainFlow-only Hub | Multi-backend (LSL, Serial, BrainFlow synthetic + native) is core value; BrainFlow is first-class, not only |
| Full framework repo split in v1.1 | PROJECT.md: full split planned for later milestone |
| New E2E framework (e.g. Playwright for Hub) | Use existing Rust integration tests and validation harness where valuable |
| 100% coverage target | Target critical paths and boundaries; coverage for visibility, not single goal |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| TEST-01 | Phase 8 | Complete |
| TEST-02 | Phase 8 | Complete |
| TEST-03 | Phase 8 | Complete |
| TEST-04 | Phase 8 | Complete |
| TEST-05 | Phase 8 | Complete |
| FRAME-01 | Phase 7 | Complete |
| FRAME-02 | Phase 7 | Complete |
| FRAME-03 | Phase 7 | Complete |
| FRAME-04 | Phase 7 | Complete |
| BRAIN-01 | Phase 9 | Pending |
| BRAIN-02 | Phase 9 | Pending |
| BRAIN-03 | Phase 9 | Pending |
| BRAIN-04 | Phase 9 | Pending |
| BRAIN-05 | Phase 9 | Pending |
| BRAIN-06 | Phase 10 | Pending |
| BRAIN-07 | Phase 10 | Pending |
| BRAIN-08 | Phase 10 | Pending |

**Coverage:**
- v1.1 requirements: 17 total
- Mapped to phases: 17
- Unmapped: 0 ✓

---
*Requirements defined: 2026-02-21*
*Last updated: 2026-02-21 after v1.1 research*
