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
- `set_signal_config`

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
The `set_signal_config` command is additive and accepts the same `SignalConfig`
shape used in `SystemConfig.signal`.

`set_signal_config` is supported in both embedded and external runtime modes:

- Embedded mode: forwarded to the in-process signal task
- External mode: forwarded through control transport and applied without service restart

Representative `set_signal_config` request:

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

## Snapshot Additive Fields

The runtime snapshot contract remains backward-compatible and now includes
additional optional/additive telemetry fields:

- `ControlSnapshot`:
  - `pipeline_integrity_degraded` (default `false`)
  - `integrity_issue_count` (default `0`)
  - `stage_health_summary` (`null` when unavailable)
    - Human-readable rollup string (for example:
      `pipeline:ok[normal] device:ok(0) signal:degraded(2) decoder:ok(0) action:ok(0) ipc:ok(0)`)
    - Intended for UI/operator diagnostics, not strict machine parsing

- `DiscoveredStream` optional runtime telemetry:
  - `effective_sample_rate_hz`
  - `samples_received`
  - `samples_dropped`
  - `drop_rate_pct`
  - `last_sample_age_ms`
  - `preprocessing_summary`
  - `integrity_state`
  - All listed fields are additive and optional (`Option<T>`), preserving
    wire compatibility with older clients.

## Observability Policy Surface

`SystemConfig.service.observability` defines global + per-component rate controls
for structured tracing.

- Global policy: baseline defaults merged into component policy
- Per-component policy keys:
  - `device`
  - `signal`
  - `decoder`
  - `action`
  - `ipc`
  - `control`

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
