# API and Control Contracts (`rust-core`)

## Scope

This project primarily exposes local control and bridge protocol contracts rather than external HTTP APIs.

## Control Command Surface

The runtime supports a local control request model with command types including:

- `snapshot`
- `shutdown`
- `set_calibration_mode`
- `set_output_enabled`
- `reload_model`
- `promote_candidate_model`
- `rescan_streams`
- `connect_stream`
- `disconnect_stream`
- `set_learning_enabled`
- `ml_bridge_reconnect`
- `trainer_snapshot`
- `set_fallback_policy`

Representative control request shape:

```json
{
  "request_id": "1",
  "command": {
    "type": "snapshot"
  }
}
```

Representative response surface includes runtime snapshot and trainer snapshot structures.

## Bridge Protocol Surface

Local envelope-based messaging is used for runtime ↔ Python communication.
Representative envelope fields include:

- `v` (version)
- `kind` (message kind)
- `seq` (sequence id)
- `sent_at_us` (timestamp)
- `session_id`
- `payload`

Common message kinds include handshake/health (`hello`, `ping`, `pong`, `ack`, `error`) and
runtime/ML domain events (decision, telemetry, ErrP/trainer outputs, candidate model status).

## Non-Goals

- No public internet-facing REST API identified in this repository scan.
- No auth-token based external API surface detected; communication is local-process oriented.
