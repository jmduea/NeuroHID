# Runtime-ML Protocol v2

This document defines the JSON envelope contract for Runtime ↔ Trainer messaging.

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
