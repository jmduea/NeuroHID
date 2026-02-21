# Phase 6: Composable and extensible - Context

**Gathered:** 2026-02-21
**Status:** Ready for planning

## Phase Boundary

Pipeline components (acquisition, signal preprocessing, decoder, output) are swappable via published contracts; new device backends and new action/output types can be added without changing core orchestration. One example plugin (device or outlet) exists and is tested in CI. Requirements: COMP-06, EXT-01, EXT-02, EXT-03. Scope is fixed; this context clarifies *how* to implement that.

## Implementation Decisions

### Area 1: How "replace" works

- **Selection:** Config is source of truth. Hub shows the list of discovered extensions when configuring device/outlet; CLI/headless stay config-only.
- **Load/run:** Either in-process (e.g. Rust/dylib) or subprocess (e.g. Python/external binary) is allowed, provided the implementation satisfies the same contract.
- **Config when picking in Hub:** Always sync — choice in Hub updates config immediately; no separate "save" for device/outlet.
- **Custom component fails to load/run:** Fail clearly, no silent fallback. Show a clear error; pipeline is stopped or degraded with an explicit reason; never substitute another component.
- **Which slots:** All four — acquisition, signal preprocessing, decoder, output. Each has a contract and selection in config/Hub.
- **Config change while runtime running:** Apply on next start of that slot (e.g. "reconnect device" / "reload outlet") where design allows, without full restart.
- **Where in Hub to pick:** Both — an Extensions screen to manage/install; Devices and outlet config still show the choice (built-in vs extension by name) in place.
- **No custom extensions:** Same UI, shorter list — provider/outlet dropdown still shown with only built-ins; no special "no extensions" state.

### Area 2: Example plugin and CI

- **Kind:** Outlet plugin — minimal custom outlet (e.g. no-op or log-only) implementing the outlet contract (EXT-02).
- **How "real":** Planner decides — pick minimal vs slightly realistic from CI/docs needs.
- **CI for "tested in CI":** Build + end-to-end — runtime runs with the example outlet; assert something observable (e.g. actions to file, or control snapshot shows outlet type).
- **Where example lives in repo:** Planner decides — workspace member vs separate directory from CI/docs clarity.

### Area 3: What "still integrate" means

- **Control:** Unchanged — snapshot and other control work the same; custom component appears in snapshot (e.g. "outlet: custom" or by name) and can be toggled like built-ins.
- **Hub:** Full parity — strip, visualization, config, Run all work with custom component like built-ins (e.g. strip shows "Custom outlet" or name; visualization works when slot produces observable data).
- **Observability:** Same as built-ins — integrity/snapshot reflects the custom component (e.g. "outlet: ok" or by name); session recording captures pipeline state including that slot; logs treat it like any other component.
- **Runtime (headless):** Full parity — headless service starts, runs, and shuts down the same as with built-ins; custom component is just another pipeline slot; no special modes or limitations.

### Area 4: How extensions are discovered and chosen

- **Where runtime looks for extensions:** Planner decides — pick discovery strategy (e.g. fixed dir, configurable paths, default + override) and document it.
- **How each extension is identified:** By name only — extension declares a string name (e.g. in a manifest); that name is the sole ID; version is not part of the ID.
- **Duplicate names:** Fail — treat duplicate extension names as an error (invalid config/discovery); do not start or load that extension.
- **When to rescan:** Startup + on demand — discover at start; also support explicit refresh (Hub/CLI) so new extensions appear without full restart.

## Specific Ideas

- Contracts live in neurohid-types or published docs; each slot (device provider, signal pipeline, decoder, outlet) has a defined contract.
- Example outlet plugin demonstrates EXT-02 and is the basis for CI "tested in CI"; planner chooses exact placement and minimal vs slightly realistic behavior.

## Deferred Ideas

- Version in extension ID / selection (v1: name-only; version as metadata for display or compatibility can be added later if needed).
- Plugin discovery/lifecycle fully specified and documented is EXT-04 (v2); v1 implements discovery and refresh per above.

---

*Phase: 06-composable-and-extensible*
*Context gathered: 2026-02-21*
