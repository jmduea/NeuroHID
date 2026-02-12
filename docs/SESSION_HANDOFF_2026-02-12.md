# Session Handoff — 2026-02-12

## Current status

- Workspace compiles: `cargo check --workspace` ✅
- Core tests pass: `cargo test -p neurohid-core` ✅
- Hub IPC transition test passes: `cargo test -p neurohid-hub snapshot_tracks_real_ipc_connect_disconnect_transitions -- --nocapture` ✅
- IPC remains simulated by default (intentional): `service.ipc_simulation_enabled = true`

## What changed (grouped)

### 1) Docs + changelog alignment
- Updated root docs for current run commands, Python setup, and roadmap/licensing notes.
- Added IPC simulation-mode documentation.
- Expanded unreleased changelog entries to reflect implemented behavior.

### 2) Runtime safety hardening
- Removed panic-prone default behavior in secure storage fallback path.
- Hardened hub initialization fallback to avoid startup panic chains.

### 3) Core signal/IPC behavior
- Signal task refactor:
  - `VecDeque` ring-buffer semantics
  - timestamp-based sampling cadence estimation
  - duration-to-sample helper + tests
- IPC task refactor:
  - config gate for simulation mode
  - explicit state propagation (`ipc_connected`, `ipc_simulated`)
  - tests for disabled simulation path and simulated lifecycle

### 4) Hub state/status integration
- Service snapshot now includes `ipc_simulated`.
- Sidebar displays IPC status: `Connected` / `Simulated` / `Disconnected`.
- Snapshot caching adjusted to avoid transient empty-state regressions during lock contention.

### 5) Placeholder-safe warning hygiene
- Preserved intentional future fields while reducing explicit dead-code allowances (underscore-prefix strategy).

### 6) Real bridge + transition verification
- `neurohid-core` IPC task now supports real TCP bridge mode when `ipc_simulation_enabled = false`.
- Simulation mode remains the default fallback path.
- Added real-bridge tests for:
  - action forwarding from Python to Rust
  - disconnect/reconnect lifecycle handling
- Added hub-side service manager test that verifies snapshot transitions against real bridge events.

## Suggested commit split

1. `docs: align commands, python setup, and IPC mode docs`
   - README/CONTRIBUTING/core README/changelog docs-only updates

2. `core: add IPC simulation gate and state propagation`
   - `neurohid-types` config flag + `neurohid-core` service/ipc wiring/tests

3. `core: improve signal buffering cadence and add helper tests`
   - `signal.rs` `VecDeque`/timing changes and unit tests

4. `hub: surface IPC mode in snapshot and sidebar`
   - hub state/service manager/app updates

5. `storage+cleanup: remove panic default path and placeholder-safe dead-code cleanup`
   - secure storage default + `_`-prefixed reserved fields

## First tasks for next session

1. Wire readiness/health semantics between Rust IPC and the Python bridge (`Ready`/`Ping` policy and timeout behavior).
2. Add coverage for ErrP result flow over real IPC (not just action flow).
3. Decide whether service startup should optionally spawn/monitor the Python process lifecycle.
4. Prepare and execute the intended commit split now that IPC + hub transition items are complete.

## Quick restart commands

```bash
cargo check --workspace
cargo test -p neurohid-core
cargo run -p neurohid --bin neurohid-service
cargo run -p neurohid --bin neurohid
```

## Notes

- There are additional hub/widget deltas in the working tree beyond the core IPC/signal/docs slice; include or split them intentionally during commit prep.
- No commits or branch operations were performed in this session.
