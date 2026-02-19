# Protocol and API Reference

This document is the single source of truth for the NeuroHID IPC protocol,
control commands, and observability contracts.

## IPC v3 Envelope

All communication uses JSON envelopes over local transport (named pipe on
Windows, loopback TCP elsewhere).

```json
{
  "v": 3,
  "channel": "control.rpc",
  "msg_type": "request",
  "seq": 1,
  "request_id": "req-1",
  "session_id": "session-1",
  "sent_at_us": 1739596800000000,
  "payload": {}
}
```

- `v` must be `3`.
- `channel` identifies the logical stream.
- `msg_type` is channel-specific.

## Channels

| Channel | Direction | Purpose |
|---|---|---|
| `control.rpc` | Request/response | Service control commands |
| `trainer.stream` | Bidirectional | Runtime ↔ trainer ML messages |
| `runtime.events` | Runtime → observers | Event stream for UI/notebook clients |

## `control.rpc`

Request: `msg_type = "request"`, payload shape `ControlRpcRequest`.

Response: `msg_type = "response"`, payload shape `ControlRpcResponse` with
variants: `ack`, `snapshot`, `trainer_snapshot`, `error`.

### Control Commands

- `snapshot` — current runtime state
- `shutdown` — graceful shutdown
- `set_calibration_mode` — toggle calibration
- `set_output_enabled` — enable/disable HID output
- `reload_model` — reload decoder model
- `promote_candidate_model` — promote candidate to active
- `rescan_streams` — re-discover EEG streams
- `connect_stream` / `disconnect_stream` — manage stream connections
- `set_learning_enabled` — toggle online learning
- `ml_bridge_reconnect` — force bridge reconnect
- `trainer_snapshot` — trainer state snapshot
- `set_fallback_policy` — configure fallback behavior
- `set_signal_config` — update signal processing config at runtime

`set_signal_config` accepts the same `SignalConfig` shape used in
`SystemConfig.signal` and works in both embedded and external runtime modes.

Example:

```json
{
  "request_id": "42",
  "command": {
    "type": "set_signal_config",
    "signal": {
      "buffer_size_samples": 1024,
      "notch_filter_enabled": true,
      "notch_filter_hz": 60.0,
      "bandpass_filter_enabled": true,
      "bandpass_low_hz": 1.0,
      "bandpass_high_hz": 40.0,
      "feature_window_ms": 500,
      "feature_step_ms": 50,
      "artifact_rejection_enabled": true,
      "artifact_threshold_uv": 100.0
    }
  }
}
```

## `trainer.stream`

Trainer stream message kinds (via `msg_type`):

`hello`, `session_boundary`, `decision_event`, `errp_window`,
`runtime_telemetry`, `ping`, `shutdown`, `errp_result`, `trainer_status`,
`candidate_model_ready`, `pong`, `ack`, `error`.

## `runtime.events`

Observer clients use:

- `msg_type = "poll"` for one-shot reads
- `msg_type = "subscribe"` for streaming subscriptions

The runtime replies with `msg_type = "event"`, payload shape `RuntimeEvent`.

### Poll

Optional `family` selector: `snapshot`, `trainer_snapshot`, `trainer_status`,
`runtime_telemetry`, `capabilities`.

### Subscribe

Options (typed as `RuntimeEventsSubscribe`):

- `families: string[]` — event family filter
- `resume_from_seq: u64` — resume cursor
- `max_events: u64` / `max_duration_ms: u64` — stream bounds
- `sample_every: u64` — downsampling
- `snapshot_interval_ms: u64` — periodic snapshot injection
- `include_snapshot: bool` (default `true`)
- `include_capabilities: bool` (default `true`)

### Event Families

`snapshot`, `trainer_snapshot`, `trainer_status`, `runtime_telemetry`, `sample`,
`feature_frame`, `observation_frame`, `action_emitted`, `marker`,
`decision_event`, `errp_window`, `errp_result`, `integrity_issue`,
`backpressure_drop`, `capabilities`, `lifecycle`.

### Replay/Resume Semantics

If `resume_from_seq` is within the replay window, the runtime emits historical
events with original `seq` values, then a `lifecycle` event with
`state = "replay_resumed"` and structured metadata (`requested_seq`,
`replay_window_start_seq`, `replay_window_end_seq`).

If the cursor is outside the replay window, the runtime emits
`state = "replay_miss"` instead. Clients must treat this as a resync trigger:
refresh state via `control.rpc/snapshot` and restart the stream.

Replay window defaults: `10,000` events or `120s` retention.

### Capabilities

`type = "capabilities"` payload:

- `observation_schema_version: u16`
- `channels: IpcChannel[]`
- `components: RuntimeComponentCapability[]` (`name`, `available`,
  optional `unavailable_reason`)

## Snapshot Contract

The `ControlSnapshot` includes additive telemetry fields:

- `pipeline_integrity_degraded` (default `false`)
- `integrity_issue_count` (default `0`)
- `stage_health_summary` (nullable human-readable rollup)

`DiscoveredStream` optional runtime telemetry (all `Option<T>`):

- `effective_sample_rate_hz`, `samples_received`, `samples_dropped`
- `drop_rate_pct`, `last_sample_age_ms`
- `preprocessing_summary`, `integrity_state`

## Integrity Enforcement

Runtime enforces trainer-stream sequence and payload sanity:

- `seq` must be strictly increasing per session
- Sequence regressions/gaps are flagged as integrity issues
- Invalid/non-finite critical payload fields are rejected

These checks are non-fatal by default (warn + degrade policy).

## Observability Policy

`SystemConfig.service.observability` defines global + per-component rate
controls for structured tracing.

Per-component policy keys: `device`, `signal`, `decoder`, `action`, `ipc`,
`control`.

## Non-Goals

- No public internet-facing REST API.
- No auth-token based external API surface.
- Communication is local-process oriented.
