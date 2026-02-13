# NeuroHID V1 Remaining Tasks

This checklist captures what is still open after the Runtime ML Protocol v2 + named-pipe cutover and Python Lab always-on monitor integration.

## Priority 0 (release blockers)

- [x] Fix DSP window sizing edge case that still causes fallback warnings (`window has 63 samples, need at least 64 for Welch PSD`) in live runs.
  - Target: no repeated feature-extraction fallback spam under nominal 128 Hz streams.
  - Suggested area: `crates/neurohid-core/src/tasks/signal.rs` window/step calculation and extractor input sizing.

- [x] Complete runtime-generated `errp_window` payload path.
  - Current path sends `decision_event` and receives `errp_result`, but explicit ErrP windows are not yet emitted from runtime bridge.
  - Target: runtime emits `errp_window` with correlated `decision_id`, trainer responds with `errp_result`.

- [x] Harden candidate model promotion security/validation for external trainer-provided artifact paths.
  - Current path imports from `candidate_model_ready.artifact_dir` then promotes.
  - Target: restrict import roots, enforce path policy, and add additional manifest/metrics sanity checks before import.

- [x] Add end-to-end Windows named-pipe integration tests (control + ML channels).
  - Include connect/disconnect/reconnect and bridge stall/recovery transitions.

## Priority 1 (important for v1 quality)

- [x] Add Hub controls for v2 control commands:
  - `SetLearningEnabled`
  - `MlBridgeReconnect`
  - `SetFallbackPolicy`
  - `TrainerSnapshot` display

- [x] Add explicit desktop/user notifications for runtime mode transitions (`full` <-> `fallback` <-> `degraded`) in Hub UX.
  - Runtime logs transition alerts with cooldown, but UI notification flow should surface them consistently.

- [x] Replace placeholder trainer behavior in Python bridge.
  - Current bridge uses proxy/error heuristics for `decision_event` -> `errp_result`.
  - Target: real trainer/replay integration with stable status metrics.

- [x] Make external-mode named-pipe control client more robust.
  - Add retry/backoff and timeout behavior parity with TCP control path.

## Priority 2 (performance and polish)

- [x] Run the protocol encoding benchmark gate for JSON v2 vs protobuf (RFC Phase 6).
  - Only migrate payload encoding if benchmark thresholds are met.
  - 2026-02-13 gate result (`cargo run -p neurohid-ipc --bin protocol_encoding_gate`):
    - JSON: encode `93657.2 ns/msg`, decode `88664.4 ns/msg`, size `4931.0 bytes/msg`
    - Protobuf-shaped: encode `4036.8 ns/msg`, decode `10568.1 ns/msg`, size `1055.7 bytes/msg`
    - Decision: `MIGRATE_TO_PROTOBUF` (thresholds exceeded).

- [x] Add observability dashboards for trainer metrics over time.
  - Replay size, training step, losses/entropy trends, candidate promote/reject outcomes.
  - Implemented in Hub Dashboard with rolling sparklines and candidate outcome history.

- [x] Resolve remaining low-value warnings in hub widgets (`unused doc comment` style warnings).

## Validation matrix to complete

- [ ] 24h soak test with forced bridge restarts.
- [ ] Full/fallback/degraded latency comparison (p95 decode/action).
- [ ] Resource ceilings (CPU/RAM) in all runtime modes.
- [ ] No-Python-bridge boot scenarios:
  - ONNX present -> `full` when bridge healthy / `fallback` when absent
  - ONNX absent + lightweight model -> `fallback`
  - No usable model -> `degraded`

## Notes

- `cargo check` and `cargo test` pass on current tree.
- Python Lab now includes an always-on live feature monitor (from DataBus feature stream).
