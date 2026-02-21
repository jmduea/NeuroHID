# Roadmap: NeuroHID

## Milestones

- ✅ **v1.0 MVP** — Phases 1–6 (shipped 2026-02-21). Full detail: [milestones/v1.0-ROADMAP.md](milestones/v1.0-ROADMAP.md)
- 🚧 **v1.1 Testing, BrainFlow & Framework Separation** — Phases 7–10 (in progress)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1–6) — SHIPPED 2026-02-21</summary>

Phases 1–6 completed; see [milestones/v1.0-ROADMAP.md](milestones/v1.0-ROADMAP.md) for full detail.

</details>

### 🚧 v1.1 Testing, BrainFlow & Framework Separation (In Progress)

**Milestone Goal:** Improve confidence and developer clarity: thorough testing, first-class BrainFlow integration (docs/UX then deeper), and a clear structural split between the reusable framework and the NeuroHID Hub application.

- [ ] **Phase 7: Framework–Hub Separation** — Structural boundary and docs; framework surface documented; Hub depends only on core/facade; CI or audit enforces boundary
- [ ] **Phase 8: Thorough Testing** — Deterministic tests, integration at boundaries, CI gates that reflect reality, one valuable E2E path, test tiers documented
- [ ] **Phase 9: BrainFlow First-Class** — Docs, runnable examples (synthetic board), Hub discover/connect UX; synthetic board replaces in-repo mock everywhere
- [ ] **Phase 10: BrainFlow Deeper** — Real SDK behind feature flag, streaming path into pipeline, pinned version and build steps

## Phase Details

### Phase 7: Framework–Hub Separation
**Goal**: Developer has a clear boundary between the framework (what to depend on) and the Hub (one application on top).
**Depends on**: Phase 6 (v1.0)
**Requirements**: FRAME-01, FRAME-02, FRAME-03, FRAME-04
**Success Criteria** (what must be TRUE):
  1. Developer can find the documented framework surface (which crates and features to depend on) and use it without depending on Hub internals.
  2. Hub is documented as one application built on the framework; dependency graph and docs define the boundary.
  3. Hub depends only on core (and calibration) and the framework facade; dependency audit or CI check enforces no disallowed direct deps from Hub to component crates.
  4. Docs describe the framework vs Hub boundary so contributors and embedders know what is framework vs application.
**Plans**: TBD

### Phase 8: Thorough Testing
**Goal**: Developer has confidence that tests are deterministic, key boundaries are covered, and CI reflects reality.
**Depends on**: Phase 7
**Requirements**: TEST-01, TEST-02, TEST-03, TEST-04, TEST-05
**Success Criteria** (what must be TRUE):
  1. Developer gets deterministic test runs (no flakiness from async/concurrency or shared state) via test policy and tooling.
  2. Developer has integration tests at key boundaries (IPC Rust↔Python, device→signal→decoder→action pipeline, config load/save) so interface mismatches are caught in CI.
  3. CI gates reflect reality (coverage and flakiness addressed) so passing CI means safe-to-merge for the scope exercised.
  4. Developer has at least one valuable E2E path (e.g. Hub discover→connect→stream or runtime profile→decoder→action) exercised in tests.
  5. Test tiers and isolation policy are documented so contributors know unit vs integration vs E2E and how to avoid flakiness.
**Plans**: TBD

### Phase 9: BrainFlow First-Class
**Goal**: User or developer can use BrainFlow with NeuroHID via first-class docs, runnable examples, and Hub UX; synthetic board fully replaces the in-repo mock device.
**Depends on**: Phase 8
**Requirements**: BRAIN-01, BRAIN-02, BRAIN-03, BRAIN-04, BRAIN-05
**Success Criteria** (what must be TRUE):
  1. User or developer can read first-class documentation for using BrainFlow with NeuroHID (setup, config, synthetic vs native hardware, build order).
  2. User can run at least one runnable example using BrainFlow's synthetic board that demonstrates BrainFlow with NeuroHID end-to-end.
  3. User can discover and connect BrainFlow devices from the Hub Devices screen with UX parity to LSL and other backends (discover, connect, disconnect).
  4. BrainFlow remains one backend behind the existing DeviceProvider/Device abstraction; device-agnostic API is preserved.
  5. BrainFlow's synthetic board fully replaces the in-repo mock device: tests, examples, and CI use the synthetic board as the single non-hardware device path (no separate mock backend).
**Plans**: TBD

### Phase 10: BrainFlow Deeper
**Goal**: Developer can build and use the real BrainFlow SDK and streaming path; builds are reproducible.
**Depends on**: Phase 9
**Requirements**: BRAIN-06, BRAIN-07, BRAIN-08
**Success Criteria** (what must be TRUE):
  1. Developer can build and use the real BrainFlow SDK in neurohid-device behind a feature flag (e.g. brainflow-native); default and CI use synthetic board only (no mock).
  2. User or developer can use the BrainFlow streaming path into the same signal pipeline as LSL and other backends (synthetic or real board → Device → pipeline).
  3. BrainFlow version and build steps (C++ core then Rust) are pinned and documented so builds are reproducible.
**Plans**: TBD

## Progress

**Execution Order:** Phases execute in numeric order: 7 → 8 → 9 → 10

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 7. Framework–Hub Separation | v1.1 | 0/? | Not started | - |
| 8. Thorough Testing | v1.1 | 0/? | Not started | - |
| 9. BrainFlow First-Class | v1.1 | 0/? | Not started | - |
| 10. BrainFlow Deeper | v1.1 | 0/? | Not started | - |

---
_Last updated: 2026-02-21 — v1.1 roadmap created_
