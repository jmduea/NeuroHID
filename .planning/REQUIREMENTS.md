# Requirements: NeuroHID v1.2

**Defined:** 2026-02-22
**Core Value:** A single, composable path from biosignal device to actionable output — with an IDE-like experience for building and training decoders and a standalone runtime for using them — so that developers and power users don't have to piece together disparate libraries and tools.

## v1.2 Requirements

Requirements for milestone v1.2 (Framework consumable — prep for Python bindings). Each maps to roadmap phases.

### Framework publishable & consumable

- [ ] **FRAME-06**: Developer can publish every framework-surface crate to a registry; `cargo publish -p <crate> --dry-run` passes for each; path+version deps and publishable metadata (description, license, repository) for each publishable crate
- [ ] **FRAME-07**: Hub (and binaries/SDK) depend on framework crates via path+version or version; allowlist unchanged; same API surface; binaries (neurohid, neurohid-service, neurohid-validate) still build
- [ ] **FRAME-08**: Framework is consumable without building from local path — embedders and Hub can depend on version or tag; framework crates have compatible versions (single coherent framework unit)

### Release story

- [ ] **REL-01**: SemVer versioning for framework crates; single source of truth (e.g. workspace.package); version bumps follow SemVer for public API
- [ ] **REL-02**: Changelog and git tag per release; documented publish order (types → components → core); optional release automation (release-plz or cargo-release)
- [ ] **REL-03**: CI runs `cargo publish -p <crate> --dry-run` for every publishable framework crate so publish readiness is verified

### Prep for Python bindings (API boundary only)

- [ ] **BIND-01**: Single embedder-facing surface documented; audit complete so no internal types leak into that public API (core facade / SDK)
- [ ] **BIND-02**: Documented "intended bindable surface" for future bindings milestone; no PyO3/maturin/cdylib or Python-facing code in v1.2

## Future requirements (v1.3+)

Deferred to future milestones. Tracked but not in current roadmap.

### Framework

- **FRAME-09**: Framework in separate repo; main repo consumes via git tag or crates.io (optional layout after publishable-in-place is stable)

### Python bindings

- **BIND-03**: Python bindings milestone — PyO3/maturin, scriptable runtime from Python, PyPI package, tighter ML bridge

## Out of Scope

Explicitly excluded for v1.2. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| PyO3, maturin, cdylib, or PyPI in v1.2 | Prep only; bindings in later milestone |
| Framework as single giant crate | Preserve component boundaries and existing neurohid-* crates |
| Only path deps forever | v1.2 delivers versioned consumable framework |
| Splitting repo without publishing | Consumable = version or git ref; publish or tag required |
| Private registry as v1.2 deliverable | Can add later if not using crates.io |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| FRAME-06 | — | Pending |
| FRAME-07 | — | Pending |
| FRAME-08 | — | Pending |
| REL-01 | — | Pending |
| REL-02 | — | Pending |
| REL-03 | — | Pending |
| BIND-01 | — | Pending |
| BIND-02 | — | Pending |

**Coverage:**
- v1.2 requirements: 8 total
- Mapped to phases: 0
- Unmapped: 8 ⚠️

---
*Requirements defined: 2026-02-22*
*Last updated: 2026-02-22 after research synthesis*
