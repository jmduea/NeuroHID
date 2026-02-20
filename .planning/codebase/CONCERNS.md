# Codebase Concerns

**Analysis Date:** 2026-02-20

## Tech Debt

**Data bus and widget constants:**
- Issue: Ring-buffer sizes and signal-quality thresholds are hardcoded; data bus design choice (poll/drain from GUI thread) is marked for reconsideration.
- Files: `crates/neurohid-hub/src/data_bus.rs` (TODO lines 17, 98), `crates/neurohid-hub/src/widgets/signal_quality.rs` (TODO lines 13, 19), `crates/neurohid-hub/src/widgets/time_series.rs` (TODO line 14)
- Impact: Operators cannot tune buffer sizes or quality thresholds; GUI-thread poll design may limit scalability or latency.
- Fix approach: Add config (e.g. system or profile config) for buffer sizes and quality thresholds; consider moving drain to a dedicated task if needed. Introduce a shared style module for time-series/widget styling to avoid duplication.

**Oversized binary and app modules:**
- Issue: Single-file binaries and app logic are very large, which complicates maintenance, testing, and incremental compilation.
- Files: `crates/neurohid/src/bin/neurohid-service.rs` (~2789 lines), `crates/neurohid-hub/src/app.rs` (~2745 lines), `crates/neurohid-hub/src/service_manager.rs` (~1638 lines), `crates/neurohid-hub/src/screens/settings.rs` (~1457 lines), `crates/neurohid-core/src/service.rs` (~1270 lines), `crates/neurohid-core/src/tasks/decoder.rs` (~1163 lines)
- Impact: Harder to reason about control flow, higher risk of merge conflicts, slower iteration.
- Fix approach: Extract cohesive subsystems into modules or crates (e.g. service CLI vs runtime host, app screens vs global state, decoder helpers vs main task). Split by responsibility and dependency boundaries per `docs/crate-boundaries.md`.

**unwrap/expect in library and binary entry:**
- Issue: Project policy prefers `Result`/`?` over `unwrap()` in library paths; binaries still use `.expect()` for startup (e.g. tokio runtime build) and tests use many `.expect()`/`.unwrap()`.
- Files: Widespread in tests (e.g. `crates/neurohid-storage/src/paths.rs`, `crates/neurohid-storage/src/config.rs`, `crates/neurohid-ipc/src/server.rs`, `crates/neurohid-hub/src/service_manager.rs`, `crates/neurohid/src/bin/neurohid-service.rs`). Production entry: `crates/neurohid/src/bin/neurohid.rs` (tokio runtime `.expect("Failed to create tokio runtime")`).
- Impact: Test-only unwraps are acceptable but can hide missing error paths; a failing runtime build panics in the Hub binary.
- Fix approach: Keep test unwraps where they clarify test intent; in library code paths avoid new unwraps and replace existing ones with `?` and typed errors where practical. For binary startup, consider logging and exit-with-code instead of panic if policy tightens.

## Known Bugs

**No user-facing bugs documented in code or docs.** Probabilistic and timeout-based test failures can appear as flaky CI:

- Tests that panic on timeout or missing outcome can fail under load or slow CI.
- Files/triggers: `crates/neurohid-calibration/src/games/grid_maze.rs` (panic if no error/correct trial in 200 iterations; lines 688, 702), `crates/neurohid-hub/src/service_manager.rs` (panic in test helper if snapshot predicate not met before timeout; line 1375), `crates/neurohid-core/src/runtime.rs` (panic in test helper on timeout; line 600), `crates/neurohid/src/bin/neurohid-service.rs` (panic in capabilities tests if event shape changes; lines 2829, 2862, 2869).
- Workaround: Re-run tests; increase timeouts or iteration counts if CI is consistently slow.

## Security Considerations

**Secure storage and keychain:**
- Risk: Key material or decrypted data could leak via logs, core dumps, or misuse of keyring APIs.
- Files: `crates/neurohid-storage/src/secure.rs` (keyring, AES-256-GCM, nonce handling).
- Current mitigation: Keyring service per platform; encrypted files; on Unix, restrictive permissions (0o600) set after write.
- Recommendations: Avoid logging key material or plaintext; keep key handling in minimal scope; consider secret zeroing where feasible.

**Unsafe and FFI usage:**
- Risk: Incorrect unsafe contracts could cause UB or misuse of platform APIs.
- Files: `crates/neurohid/src/bin/neurohid.rs` (env set before threads; WSL X11 backend), `crates/neurohid-platform/src/windows.rs` (GetCursorPos, GetSystemMetrics), `crates/neurohid-platform/src/macos.rs` (AXIsProcessTrustedWithOptions), `crates/neurohid-core/src/tasks/outlet.rs` (Send impl), `crates/neurohid-device/src/lsl/device.rs` (Send/Sync impls).
- Current mitigation: Each unsafe block has a `// SAFETY:` comment; platform calls are narrow and documented.
- Recommendations: Keep unsafe minimal; any new unsafe must include a SAFETY rationale per `crates/AGENTS.md`.

**Secrets and env:**
- No `.env` or credential files are read by the analyzer. Runtime config uses env for log format, service binary path, and notifications; no secrets are quoted in codebase docs.

## Performance Bottlenecks

**No specific slow operations identified from static analysis.** Potential areas to profile if issues appear:

- Data bus: Single-thread poll of broadcast receivers and ring buffers in `crates/neurohid-hub/src/data_bus.rs`; if widget count or message rate grows, consider batching or background drain.
- Decoder and signal tasks: Large modules (`crates/neurohid-core/src/tasks/decoder.rs`, `crates/neurohid-core/src/tasks/signal.rs`); hot paths (sample in, feature out, action out) are good candidates for profiling under load.
- Improvement path: Add targeted benchmarks or tracing; use existing `tracing` and observability hooks to find latency spikes.

## Fragile Areas

**Probabilistic and timeout-dependent tests:**
- Files: `crates/neurohid-calibration/src/games/grid_maze.rs`, `crates/neurohid-hub/src/service_manager.rs`, `crates/neurohid-core/src/runtime.rs`, `crates/neurohid/src/bin/neurohid-service.rs` (capabilities test expectations).
- Why fragile: Grid-maze tests rely on randomness (error vs correct trial within 200 runs); service_manager and runtime tests rely on state reaching a predicate within a fixed timeout; neurohid-service tests assert exact capability event shape.
- Safe modification: When changing runtime state machine or capability format, update the corresponding tests and timeouts. For grid_maze, consider seeding the RNG in tests or increasing iterations and documenting acceptable flake rate.
- Test coverage: Integration-style tests are present; unit coverage varies by crate.

**Hub app and service manager state:**
- Files: `crates/neurohid-hub/src/app.rs`, `crates/neurohid-hub/src/state.rs`, `crates/neurohid-hub/src/service_manager.rs`.
- Why fragile: Large, stateful modules with many code paths (embedded vs external service, connection lifecycle, snapshot updates). Changes to state shape or event flow can break UI or service attachment.
- Safe modification: Run Hub and service_manager tests after any state or event change; follow existing patterns for snapshot updates and command handling.
- Test coverage: service_manager has substantial tests; app has unit tests for specific helpers (e.g. device health rows, capabilities).

**IPC and protocol encoding:**
- Files: `crates/neurohid-ipc/src/server.rs`, `crates/neurohid-ipc/src/protocol.rs`, `crates/neurohid-ipc/src/broker.rs`; Python `python/src/neurohid_ml/ipc.py`, `python/src/neurohid_ml/ipc_constants.py`.
- Why fragile: Rust and Python must stay in sync on protocol version and message shapes; contract is documented in `docs/protocol-and-api.md`. Encoding/decoding mismatches can cause runtime or test failures.
- Safe modification: Update protocol docs and both stacks when changing envelope or RPC shapes; run IPC and integration tests.

## Scaling Limits

**Data bus ring buffers:**
- Current capacity: Fixed constants in `crates/neurohid-hub/src/data_bus.rs` (MAX_SAMPLES 1280, MAX_FEATURES 200, MAX_ACTIONS 200, MAX_MARKERS 512).
- Limit: Longer or higher-rate sessions will overwrite history; multiple streams use the same caps for “all sources” buffer.
- Scaling path: Make buffer sizes configurable (see Tech Debt) and/or derive from sample rate and desired window.

**Single runtime per process:**
- Design: One NeuroHID runtime per Hub or service process; no multi-tenant or multi-runtime abstraction in the current codebase.
- Limit: Scaling out requires multiple processes or future architectural change.
- Scaling path: Document as intentional; if multi-instance is needed, introduce clear process/port boundaries and config.

## Dependencies at Risk

**LSL (Lab Streaming Layer):**
- Risk: `lsl-sys` is patched from git in workspace `[patch.crates-io]`; upstream may diverge or break.
- Impact: LSL device backend in `crates/neurohid-device` depends on it; build or runtime failures on LSL paths.
- Migration plan: Track upstream releases; consider vendoring a known-good revision or contributing patches upstream.

**Unused workspace dependency:**
- Risk: `tokio-tungstenite` (native-tls) is present in workspace but not yet used in code (per STACK.md).
- Impact: Unnecessary build time and dependency surface until a feature needs it.
- Migration plan: Remove from workspace if WebSocket support is deferred, or add a feature gate and use it when implementing the feature.

## Missing Critical Features

**Not inferred as blocking current scope.** Gaps that may matter for production or research use:

- Configurable data-bus and widget thresholds (see Tech Debt).
- No formal E2E test suite: integration tests are in Rust (service_manager, neurohid-service, IPC); Python tests exercise bridge, decoder, trainer, CLI. Browser or desktop E2E not present.
- WebSocket usage is not implemented despite dependency presence.

## Test Coverage Gaps

**Python:**
- What’s not tested: Full coverage not enumerated; optional dev deps (pytest-cov, branch=true, source neurohid_ml) suggest coverage is measured. Seven test files under `python/tests/` vs multiple modules in `python/src/neurohid_ml/`.
- Files: `python/src/neurohid_ml/` (notebook, telemetry, control, bridge, decoder, trainer, lab_kernel, ipc, cli).
- Risk: Untested branches in IPC, control, or trainer could break in edge cases.
- Priority: Medium; run `uv run --project python pytest python/tests` and coverage report to identify gaps.

**Rust:**
- Unit tests are spread across crates (`#[cfg(test)]` in many libs); integration-style tests concentrate in neurohid-service and service_manager. Decoder, signal, device, storage, and IPC have tests but not uniform depth.
- Risk: Refactors in core tasks (decoder, signal, device) could miss regressions if tests are narrow.
- Priority: Medium; add unit tests for new library code and critical paths; run `cargo test --workspace` and consider coverage (e.g. tarpaulin) for hot crates.

---

*Concerns audit: 2026-02-20*
