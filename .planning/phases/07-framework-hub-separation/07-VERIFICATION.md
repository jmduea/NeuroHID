---
phase: 07-framework-hub-separation
verified: "2026-02-21T00:00:00Z"
status: passed
score: 5/5 must-haves verified
---

# Phase 7: Framework–Hub Separation Verification Report

**Phase Goal:** Developer has a clear boundary between the framework (what to depend on) and the Hub (one application on top).

**Verified:** 2026-02-21  
**Status:** passed  
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #   | Truth                                                                                  | Status     | Evidence |
| --- | -------------------------------------------------------------------------------------- | ---------- | -------- |
| 1   | Developer can find a single doc that lists the framework surface (which crates/features to depend on) | ✓ VERIFIED | `docs/framework-surface.md` exists; "Framework surface" section with layer table (types, components, orchestration, applications); embedder guidance and link to allowlist. |
| 2   | Docs state Hub is one application built on the framework; dependency graph defines the boundary | ✓ VERIFIED | Same doc: "Hub is one application", mental model, conceptual map, allowlist defined in `.github/framework-allowlist.toml` and enforced by CI. |
| 3   | Contributors and embedders can read what is framework vs application                     | ✓ VERIFIED | `docs/index.md` links to framework-surface under "Architecture and System Docs"; `README.md` links in Architecture; `docs/crate-boundaries.md` subsection points to framework-surface and allowlist. |
| 4   | Hub depends only on allowlisted crates; no direct path deps on component crates outside the list | ✓ VERIFIED | `neurohid-hub/Cargo.toml` path deps: neurohid-types, neurohid-core, neurohid-ipc, neurohid-calibration, neurohid-storage — all in allowlist. Script run exits 0. |
| 5   | Adding a disallowed direct dep to Hub (or binaries) fails CI until fixed                 | ✓ VERIFIED | `.github/scripts/check-framework-boundary.ps1` reads allowlist from TOML, uses `cargo metadata` to get path deps for neurohid-hub and neurohid, exits 1 with stderr on violation. CI job `framework-boundary` runs script with pwsh and fails run on non-zero exit. |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `docs/framework-surface.md` | Framework surface and Hub boundary; embedder-oriented | ✓ VERIFIED | Exists; mental model, framework surface table, Hub boundary, allowlist reference, guidance, conceptual map; references `.github/framework-allowlist.toml`. |
| `.github/framework-allowlist.toml` | Canonical allowlist for CI; [hub] and [binaries] | ✓ VERIFIED | Exists; `[hub]` allowed: neurohid-types, neurohid-core, neurohid-calibration, neurohid-storage, neurohid-ipc; `[binaries]` adds neurohid-hub. |
| `docs/index.md` | Doc index linking to framework boundary | ✓ VERIFIED | Line 20: "Framework surface and Hub boundary (what to depend on): [framework-surface.md](framework-surface.md)". |
| `README.md` | Product intro with link to framework boundary for embedders | ✓ VERIFIED | Line 52: "Building another app? See [Framework surface and Hub boundary](docs/framework-surface.md)." |
| `docs/crate-boundaries.md` | Relationship to framework surface | ✓ VERIFIED | "Framework surface and Hub boundary" subsection (lines 35–37) points to framework-surface.md and allowlist; defers allowlist to that doc. |
| `.github/scripts/check-framework-boundary.ps1` | Script enforcing Hub/binaries path deps ⊆ allowlist | ✓ VERIFIED | Exists; reads `.github/framework-allowlist.toml`, parses [hub]/[binaries].allowed, uses cargo metadata, asserts path deps for neurohid-hub and neurohid; exit 1 on violation. Ran locally: exit 0. |
| `.github/workflows/ci.yml` | CI job that runs boundary check | ✓ VERIFIED | Job `framework-boundary`; needs determine-impact; if rust or automation or push; step "Check framework boundary" runs `./.github/scripts/check-framework-boundary.ps1` with pwsh. |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| docs/index.md | docs/framework-surface.md | Markdown link in Architecture section | ✓ WIRED | `[framework-surface.md](framework-surface.md)` at line 20. |
| README.md | docs/framework-surface.md | Link in Architecture section | ✓ WIRED | `[Framework surface and Hub boundary](docs/framework-surface.md)` at line 52. |
| docs/framework-surface.md | .github/framework-allowlist.toml | Documented as source of truth for CI allowlist | ✓ WIRED | "Defined in: .github/framework-allowlist.toml" and "Single source of truth" in Guidance; link in Hub boundary section. |
| check-framework-boundary.ps1 | .github/framework-allowlist.toml | Reads allowlist from TOML | ✓ WIRED | `$AllowlistPath = Join-Path $RepoRoot '.github/framework-allowlist.toml'`; `Get-Content -Path $AllowlistPath`; parses [hub] and [binaries]. |
| .github/workflows/ci.yml | check-framework-boundary.ps1 | Job step invokes script | ✓ WIRED | Step "Check framework boundary" runs `./.github/scripts/check-framework-boundary.ps1`; shell: pwsh. |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| FRAME-01 | 07-01 | Developer can identify the documented framework surface and use it without depending on Hub internals | ✓ SATISFIED | framework-surface.md defines surface and states types from component crates via neurohid_core::facade; index/README/crate-boundaries link to it. |
| FRAME-02 | 07-01 | Hub is documented as one application built on the framework; dependency graph and docs define the boundary | ✓ SATISFIED | framework-surface.md: "Hub is one application", layer map, conceptual map, allowlist; crate-boundaries and index reference it. |
| FRAME-03 | 07-02 | Hub depends only on core (and calibration) and framework facade; CI check enforces no disallowed direct deps | ✓ SATISFIED | allowlist TOML; script checks neurohid-hub and neurohid path deps via cargo metadata; CI job runs script; neurohid-hub and neurohid Cargo.toml path deps match allowlist. |
| FRAME-04 | 07-01 | Docs describe the framework vs Hub boundary for contributors and embedders | ✓ SATISFIED | framework-surface.md is the boundary doc; crate-boundaries subsection and index/README point to it; no orphaned phase requirements. |

All phase requirement IDs (FRAME-01, FRAME-02, FRAME-03, FRAME-04) are claimed by plans and satisfied in the codebase. No requirements in REQUIREMENTS.md for Phase 7 are orphaned.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| (none) | — | — | — | No TODO/FIXME/placeholder or stub implementations found in phase artifacts. |

### Human Verification Required

None. Automated checks and artifact/link verification are sufficient for this phase.

### Gaps Summary

None. All must-have truths, artifacts, and key links are present and wired. The framework boundary is documented, discoverable from index/README/crate-boundaries, and enforced by CI via the allowlist and script.

---

_Verified: 2026-02-21_  
_Verifier: Claude (gsd-verifier)_
