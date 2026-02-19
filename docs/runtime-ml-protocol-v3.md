# Runtime/IPC Protocol v3

This document defines the IPC v3 JSON envelope contract for local Runtime, Trainer, and notebook
clients.

## Envelope

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

- `control.rpc`: command request/response
- `trainer.stream`: runtime ↔ trainer bidirectional messages
- `runtime.events`: observer/subscriber event stream

## `control.rpc`

Request envelope:

- `channel = "control.rpc"`
- `msg_type = "request"`
- `payload` shape: `ControlRpcRequest` (`request_id`, `command`)

Response envelope:

- `channel = "control.rpc"`
- `msg_type = "response"`
- `payload` shape: `ControlRpcResponse` (`request_id`, `payload`)

Response payload variants:

- `ack`
- `snapshot`
- `trainer_snapshot`
- `error`

## `trainer.stream`

Trainer stream message kinds are encoded via `msg_type`:

- `hello`
- `session_boundary`
- `decision_event`
- `errp_window`
- `runtime_telemetry`
- `ping`
- `shutdown`
- `errp_result`
- `trainer_status`
- `candidate_model_ready`
- `pong`
- `ack`
- `error`

## `runtime.events`

Observer clients use:

- `channel = "runtime.events"`
- `msg_type = "poll"` for one-shot reads
- `msg_type = "subscribe"` for streaming subscriptions

The runtime replies with:

- `channel = "runtime.events"`
- `msg_type = "event"`
- `payload` shape: `RuntimeEvent`

`poll` request payload:

- optional `family` selector (`snapshot`, `trainer_snapshot`, `trainer_status`,
  `runtime_telemetry`, `capabilities`)

`subscribe` request payload:

- optional `families: string[]` filter
- optional `resume_from_seq: u64`
- optional `max_events: u64`
- optional `max_duration_ms: u64`
- optional `sample_every: u64` (downsampling)
- optional `snapshot_interval_ms: u64`
- optional `include_snapshot: bool` (default `true`)
- optional `include_capabilities: bool` (default `true`)

Typed payload contract: `RuntimeEventsSubscribe`.

Replay/resume lifecycle semantics:

- If `resume_from_seq` is within replay window, runtime emits historical events with their
  original `seq` values, then emits:
  - `type = "lifecycle"`
  - `state = "replay_resumed"`
  - `requested_seq`
  - `replay_window_start_seq`
  - `replay_window_end_seq`
- If `resume_from_seq` is outside replay window (or replay is empty), runtime emits:
  - `type = "lifecycle"`
  - `state = "replay_miss"`
  - `requested_seq`
  - `replay_window_start_seq` (nullable)
  - `replay_window_end_seq` (nullable)
- `replay_miss` is a contract-level resync signal:
  - clients must treat event continuity as broken for the requested cursor
  - clients should refresh authoritative state (`control.rpc/snapshot`) and restart stream with
    a fresh cursor
- Replay window defaults are currently bounded to `10_000` events and `120s` retention.

Current runtime event families:

- `snapshot`
- `trainer_snapshot`
- `trainer_status`
- `runtime_telemetry`
- `sample`
- `feature_frame`
- `observation_frame`
- `action_emitted`
- `marker`
- `decision_event`
- `errp_window`
- `errp_result`
- `integrity_issue`
- `backpressure_drop`
- `capabilities`
- `lifecycle`

Capabilities payload (`type = "capabilities"`):

- `observation_schema_version: u16`
- `channels: IpcChannel[]`
- `components: RuntimeComponentCapability[]`
  - `name: string`
  - `available: bool`
  - `unavailable_reason?: string`

## Integrity Expectations

Runtime enforces trainer-stream sequence and payload sanity:

- `seq` should be strictly increasing per session
- sequence regressions/gaps are flagged as integrity issues
- invalid/non-finite critical payload fields are rejected and reported

These checks are non-fatal by default (warn + degrade policy).
