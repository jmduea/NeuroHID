# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-21)

**Core value:** A single, composable path from biosignal device to actionable output — with an IDE-like experience for building and training decoders and a standalone runtime for using them — so that developers and power users don't have to piece together disparate libraries and tools.
**Current focus:** v1.1 — Phase 8 (Thorough Testing)

## Current Position

Phase: 9 of 10 (BrainFlow First-Class)
**Current Plan:** 3
**Total Plans in Phase:** 3
Plan: 09-01 complete
Status: Ready to execute
Last activity: 2026-02-21 — Completed 09-01 (BrainFlow default build + first-class docs)

Progress: [██████████░░░░░░░░░░] 50% (2 plans in v1.1)

## Performance Metrics

*Updated after each plan completion*

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| (v1.0 history in prior STATE) | — | — | — |
| Phase 07-framework-hub-separation P01 | 8min | 2 tasks | 5 files |
| Phase 07-framework-hub-separation P02 | 8 | 2 tasks | 2 files |
| Phase 08-thorough-testing P03 | 5min | 2 tasks | 1 files |
| Phase 08-thorough-testing P05 | 5 | 2 tasks | 2 files |
| Phase 08-thorough-testing P01 | 15 | 2 tasks | 2 files |
| Phase 08-thorough-testing P02 | 15 | 2 tasks | 2 files |
| Phase 08-thorough-testing P04 | 15 | 2 tasks | 2 files |
| Phase 09-brainflow-first-class P01 | 5 | 2 tasks | 3 files |
| Phase 09-brainflow-first-class P02 | 10 | 2 tasks | 4 files |
| Phase 09-brainflow-first-class P03 | 15 | 2 tasks | 7 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table. Recent: framework vs Hub structural separation and BrainFlow first-class then deeper (v1.1) — in progress.
- [Phase 07-framework-hub-separation]: Framework surface and Hub allowlist in one canonical doc; allowlist file single source for CI
- [Phase 07-framework-hub-separation]: No permanent exceptions: re-export from core or update allowlist and code together
- [Phase 07-framework-hub-separation]: Boundary check runs on rust, automation, or push; no bypass or exception list
- [Phase 08-thorough-testing]: Coverage thresholds: Python 50%, Rust 35% — doc points to ci.yml env as source of truth
- [Phase 08-thorough-testing]: Retries only for identified flaky tests; broad reruns avoided; CI reflects reality
- [Phase 08-thorough-testing]: Single doc docs/testing.md for tier definitions and isolation; development-guide links only (no duplication)
- [Phase 08-thorough-testing]: Use cargo-nextest@0.9 in CI via taiki-e/install-action; rust-coverage job unchanged
- [Phase 08-thorough-testing]: nextest.toml at repo root with retries (fixed, 2) and slow-timeout (60s, terminate-after 3)
- [Phase 08-thorough-testing]: Pipeline integration test in neurohid-core; IPC and config boundaries covered by existing CI jobs
- [Phase 08-thorough-testing]: E2E service+client runs in CI on Linux only; test skipped on Windows
- [Phase 09-brainflow-first-class]: BrainFlow in neurohid-core default features; single canonical doc docs/brainflow.md, BRAIN-04 documented
- [Phase 09-brainflow-first-class]: embedded_runtime is the one runnable example; Hub copy-only for BrainFlow parity
- [Phase 09-brainflow-first-class]: Auto fallback is BrainFlow synthetic (board_id 0); Mock not used in Auto path; SDK device tests require device-brainflow

### Pending Todos

None yet.

### Blockers/Concerns

None yet.

## Session Continuity

Last activity: 2026-02-21 — v1.1 roadmap created; phases 7–10; 17 requirements mapped.
Resume file: None
