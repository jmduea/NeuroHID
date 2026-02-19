# Runtime-ML Protocol v2

This document defines the JSON envelope contract for Runtime ↔ Trainer messaging.

> Deprecated: IPC v3 is the canonical protocol contract.
> See [`runtime-ml-protocol-v3.md`](./runtime-ml-protocol-v3.md).

## Envelope

```json
{
  "v": 2,
  "kind": "hello",
  "seq": 1,
  "sent_at_us": 1739596800000000,
  "session_id": "session-1",
  "payload": {}
}
```

- `v` must be `2`.
- `kind` must be one of the message kinds listed below.

## Message Kinds

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

## Envelope Integrity Expectations

The runtime enforces sequence sanity for inbound trainer envelopes:

- `seq` should be strictly increasing per bridge session
- Repeated or regressed sequence ids are flagged as integrity issues
- Gaps in sequence ids are flagged as integrity issues

These checks are non-fatal by default (warn + degrade), and the bridge stays
online unless higher-level runtime shutdown conditions occur.

## Payload Integrity Expectations

`errp_result` payload values are validated before downstream use:

- `error_probability` must be finite; runtime clamps to `[0.0, 1.0]`
- `classification_confidence` must be finite; runtime clamps to `[0.0, 1.0]`
- Non-finite values are treated as integrity issues and ignored for that message

Integrity issues are emitted as structured tracing events with
`event="pipeline.integrity_issue"` and `stage="ipc"`.
