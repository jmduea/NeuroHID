# NeuroHID Runtime ML Protocol v2

Status: Implemented pre-1.0 (breaking changes allowed)

## Scope
This protocol defines runtime <-> trainer messaging for NeuroHID ML workflows.

- Windows product path: named pipes
- Non-Windows development path: TCP loopback fallback
- Encoding: JSON envelopes with length-prefixed framing (u32 little-endian)

## Envelope
All messages use:

```json
{
  "v": 2,
  "kind": "decision_event",
  "seq": 42,
  "sent_at_us": 1770960400123456,
  "session_id": "1770960399634341",
  "payload": {}
}
```

Fields:
- `v`: protocol version (`2`)
- `kind`: message kind
- `seq`: monotonic sequence per connection
- `sent_at_us`: unix timestamp in microseconds
- `session_id`: runtime session id
- `payload`: kind-specific object

## Message Kinds
Runtime -> Trainer:
- `hello`
- `session_boundary`
- `decision_event`
- `errp_window`
- `runtime_telemetry`
- `ping`
- `shutdown`

Trainer -> Runtime:
- `hello`
- `errp_result`
- `trainer_status`
- `candidate_model_ready`
- `pong`
- `ack`
- `error`

## Transport Defaults
Service config defaults:
- `control_transport = "named_pipe"`
- `ml_transport = "named_pipe"`
- `control_pipe_name = "\\\\.\\pipe\\neurohid.control.v2"`
- `ml_pipe_name = "\\\\.\\pipe\\neurohid.ml.v2"`
- `ml_stall_timeout_ms = 1500`
- `ml_heartbeat_interval_ms = 500`

## Runtime Fallback Behavior
Runtime mode state machine:
- `full`: ONNX path available and trainer bridge healthy
- `fallback`: bridge stalled/disconnected and/or lightweight model path active
- `degraded`: no capabilities pass gating (HID effectively limited/disabled)

Capability gating dimensions:
- confidence threshold (decoder output)
- success threshold (`1.0 - errp_error_probability`)

Per-capability gates:
- `cursor_move`
- `click`
- `keyboard`

The runtime publishes current mode and enabled capability set via control snapshot fields.

## Candidate Activation
On `candidate_model_ready`, runtime attempts:
1. import candidate artifacts from `artifact_dir` (if profile store is available)
2. dispatch guarded promotion command to decoder task
3. rely on decoder guardrails + rollback for activation safety

## Python Bridge
`neurohid-ml bridge` now runs protocol v2 by default:
- Windows default transport: `named_pipe`
- Non-Windows default transport: `tcp_loopback`

CLI examples:

```bash
uv run --directory python neurohid-ml bridge --transport named_pipe --pipe-name \\\\.\\pipe\\neurohid.ml.v2
uv run --directory python neurohid-ml bridge --transport tcp_loopback --host 127.0.0.1 --port 47384
```
