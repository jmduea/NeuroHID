# Verified Remaining Work to Complete the Full IPC Replacement Plan

## Summary

Based on current repo state, the migration is materially advanced, but the original full replacement is **not complete** yet.  
What is already in place includes unified control/events endpoint behavior, config aliasing, daemon metadata lifecycle, and hub control-path migration to `neurohid-ipc` wrappers.  
The main remaining blockers are trainer transport ownership, broker-level multiplexing/replay/backpressure semantics, hub event-driven external mode, and full Python bridge cutover.

## Progress Update (2026-02-19)

1. Unified IPC server accept loop now handles clients concurrently so long-lived `runtime.events`
   subscribers no longer serialize all traffic.
   `crates/neurohid/src/bin/neurohid-service.rs`
2. Runtime-events replay state is now shared/synchronized across connection tasks, preserving global
   monotonic sequencing under concurrent subscribers.
   `crates/neurohid/src/bin/neurohid-service.rs`
3. Replay lifecycle signaling now carries structured metadata:
   `requested_seq`, `replay_window_start_seq`, `replay_window_end_seq`.
   `crates/neurohid-types/src/ipc.rs`  
   `crates/neurohid/src/bin/neurohid-service.rs`
4. `neurohid-validate` control path now uses `neurohid-ipc` typed client helper instead of bespoke
   raw TCP framing.
   `crates/neurohid/src/bin/neurohid-validate.rs`
5. Python TCP IPC client now parses and uses canonical `ipc_endpoint` (`host:port`) when provided,
   with legacy host/port fallback retained for compatibility.
   `python/src/neurohid_ml/ipc.py`

## Verified Current Baseline (Completed So Far)

1. Unified IPC config migration path exists and legacy fields are no longer serialized.
`crates/neurohid-types/src/config.rs:1213`
2. Daemon metadata + lock lifecycle has been added with metadata-first endpoint resolution.
`crates/neurohid/src/bin/neurohid-service.rs:1260`
3. Service enforces loopback-only TCP endpoint validation for unified IPC listener.
`crates/neurohid/src/bin/neurohid-service.rs:455`
4. Hub external control requests now use shared `neurohid-ipc` blocking wrapper.
`crates/neurohid-hub/src/service_manager.rs:793`
5. Python control/notebook/telemetry now have canonical `ipc_mode/ipc_endpoint` paths (legacy knobs still present as aliases).
`python/src/neurohid_ml/control.py:31`  
`python/src/neurohid_ml/notebook.py:31`  
`python/src/neurohid_ml/telemetry.py:23`  
`python/src/neurohid_ml/cli.py:96`
6. v3 docs are consolidated into the protocol reference.
`docs/protocol-and-api.md`

## Remaining Work Required (Must Complete for Original Full Replacement)

### P0: Single-Endpoint Transport Ownership (Core Blocker)

1. Remove trainer-side dedicated listener in `IpcTask`; it still binds `ml_transport/ml_pipe_name/ipc_port`.
`crates/neurohid-core/src/tasks/ipc.rs:186`
2. Move trainer channel transport ownership to service-side unified listener so `control.rpc`, `trainer.stream`, and `runtime.events` are all served by one endpoint.
`crates/neurohid/src/bin/neurohid-service.rs`
3. Implement broker/session manager in `neurohid-ipc` (channel routing, session registry, subscriber fanout), replacing ad-hoc connection loop behavior.
`crates/neurohid-ipc/src/server.rs`
4. Ensure service no longer treats `trainer.stream` as unsupported.
`crates/neurohid/src/bin/neurohid-service.rs:708`

### P0: Runtime Events Replay/Backpressure Contract Completion

1. Enforce explicit per-channel queue/backpressure defaults and policies (Control=256 reject, Trainer=1024 stall+warn, Events=4096 oldest-drop + drop event).
`crates/neurohid-ipc/src/protocol.rs`  
`crates/neurohid-ipc/src/server.rs`
2. Add runtime IPC observability metrics/events for connection churn, lag, drops, resume hits/misses.

### P0: Runtime Observation/Event Completeness

1. Emit runtime-native `decision_event`, `errp_window`, `errp_result`, and `integrity_issue` events end-to-end; variants exist but are not emitted today.
`crates/neurohid-types/src/ipc.rs`  
`crates/neurohid/src/bin/neurohid-service.rs:1148`
2. Add capability metadata/unavailable-reason semantics for optional observation components in stream payloads.
3. Add schema/capability negotiation contract beyond current static `capabilities` payload.

### P1: Hub External Runtime Event-Driven Mode

1. ✅ Add long-lived `runtime.events` subscription consumer in external mode and apply updates to cached snapshot/trainer state.
`crates/neurohid-hub/src/service_manager.rs:686`
2. ✅ Keep polling path only as degraded fallback when subscription unavailable.
3. ✅ Add reconnect/resume behavior in hub external subscriber.

### P1: Python Full Cutover for Bridge + Public API

1. Replace remaining public transport split knobs (`control_transport`, `control_pipe_name`, `ml_transport`, `ml_pipe_name`) as canonical API; keep only compatibility aliases.
`python/src/neurohid_ml/control.py:29`  
`python/src/neurohid_ml/notebook.py:40`
2. Complete trainer bridge cutover to canonical `ipckit` path and unified endpoint semantics (currently still custom bridge transport client and separate defaults).
`python/src/neurohid_ml/bridge/__init__.py:36`
3. Ensure Python bridge/notebook uses unified endpoint semantics for command + trainer + events without transport-branching in public surface.
4. Finalize reconnect/resume behavior parity for notebook observation subscriptions once server backfill exists.

### P1: Config/Compatibility Hard Cutover

1. Remove runtime dependence on legacy split fields after trainer migration completes.
`crates/neurohid-core/src/tasks/ipc.rs:187`
2. Remove temporary legacy CLI fallback `--daemon-command`.
`crates/neurohid/src/bin/neurohid-service.rs:170`  
`python/src/neurohid_ml/control.py:332`
3. Define and enforce Rust/Python protocol compatibility matrix in CI.

### P2: Docs + Tooling + Validation Alignment

1. Update deployment docs to stop describing split control/ML endpoints and old trainer port assumptions.
`docs/deployment-guide.md:35`
2. Update `neurohid-validate` scenarios/config shaping to unified endpoint model.
`crates/neurohid/src/bin/neurohid-validate.rs:691`
3. Ensure all canonical docs reflect v3 single-endpoint architecture and replay semantics.

## Public APIs / Interfaces Still To Finalize

1. `ServiceConfig` final external contract:

- Canonical: `ipc_mode`, `ipc_endpoint`.
- Legacy fields accepted only as parse-time aliases during defined migration window.

2. `neurohid-ipc` broker API:

- `send_control(...)`
- `open_trainer_stream(...)`
- `subscribe_runtime_events(...)`
- Replay/resume and channel backpressure policy surfaces.

3. `runtime.events` subscribe contract:

- `families`, `resume_from_seq`, `max_events`, `max_duration_ms`, `sample_every`, `snapshot_interval_ms`.
- Add replay-miss/loss signaling guarantees.

4. Python public API:

- Keep `send_command(...)`, `subscribe_events(...)`, `subscribe_observations(...)`, reconnect/resume helpers.
- Demote legacy transport args to compatibility-only and remove from canonical docs/examples.

5. Daemon CLI:

- Canonical `neurohid-service daemon start|stop|status` only after fallback removal.

## Test Cases and Acceptance Scenarios Still Required

1. Broker integration tests:

- one-endpoint multiplex (`control.rpc`, `trainer.stream`, `runtime.events`) with concurrent clients.

2. Replay/resume tests:

- in-window resume hit
- out-of-window resume miss with lifecycle signal.

3. Backpressure policy tests:

- per-channel limit enforcement and drop notification behavior.

4. Runtime integration tests:

- decision->ErrP flow and integrity issues visible on `runtime.events`.

5. Hub external mode tests:

- event-driven state updates + polling fallback.

6. Python tests:

- unified client command+stream path
- reconnect/resume cursor handling with true server backfill.

7. End-to-end scenarios:

- background runtime + trainer + notebook subscriber simultaneously on one endpoint.
- control commands and observation stream with no transport-specific branching.

8. CI gates:

- Rust check/test/clippy/fmt for touched crates.
- Python pytest/ruff/black/mypy for touched package.
- Linux + Windows matrix for local socket/named-pipe behavior.

## Assumptions and Defaults

1. Breaking cutover remains accepted.
2. JSON remains wire format.
3. Local-only IPC remains enforced (`local_socket` or loopback TCP only).
4. Replay buffer defaults remain `10,000 events OR 120s`.
5. Queue defaults remain `Control=256`, `Trainer=1024`, `Events=4096/subscriber`.
6. Legacy config fields remain parse-compatible only for migration window, then removed.
7. Rust `ipckit` and Python `ipckit` stay pinned and CI-validated together.
