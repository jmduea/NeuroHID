# Deployment and Operations Guide

This guide is the canonical reference for runtime operation, transport configuration,
observability, and validation workflows.

## Runtime Modes

- Interactive desktop mode via `neurohid` binary
- Headless service mode via `neurohid-service`
- Validation mode via `neurohid-validate`

## Service Operation

Typical operations (Windows service workflow is supported by the binary):

```bash
cargo run --release -p neurohid --bin neurohid-service -- --service-command install
cargo run --release -p neurohid --bin neurohid-service -- --service-command start
cargo run --release -p neurohid --bin neurohid-service -- --service-command status
cargo run --release -p neurohid --bin neurohid-service -- --service-command stop
cargo run --release -p neurohid --bin neurohid-service -- --service-command uninstall
```

To run interactively without installing a service:

```bash
cargo run --release -p neurohid --bin neurohid-service
```

## Standalone runtime (without Hub)

You can run the standalone runtime with a chosen profile and have it run without the Hub GUI. The decoder is loaded from that profile — there is no separate "attach decoder" step; the profile implies the decoder.

**Startup options:**

- `--config <path>` — Optional. Path to configuration file; uses default location if omitted.
- `--profile <name>` — Optional. Profile to use; uses default profile if omitted.
- `--control-port <port>` — Optional. Bind the control RPC endpoint to this TCP port on 127.0.0.1.

When the config does not set `service.ipc_endpoint` and `--control-port` is not passed, the service defaults to TCP **127.0.0.1:47384** for the control server. Status (snapshot) and output toggle (`set_output_enabled`) are then available without opening the Hub. See [Local Control Endpoint](#local-control-endpoint) for the request envelope format and supported commands (`snapshot`, `set_output_enabled`). Control is available for runtime status and output toggle without the Hub.

## Transport Configuration

IPC v3 uses a single local-only endpoint for `control.rpc`, `trainer.stream`, and `runtime.events`.
Named/local-socket transports are Windows-focused. On Linux/macOS, use loopback TCP:

```toml
[service]
ipc_mode = "tcp_loopback"
ipc_endpoint = "127.0.0.1:47384"
```

Run the Python bridge against the same canonical endpoint:

```bash
uv run --directory python neurohid-ml bridge --ipc-mode tcp_loopback --ipc-endpoint 127.0.0.1:47384
```

## Local Control Endpoint

Optional control endpoint exposure:

```bash
cargo run --release -p neurohid --bin neurohid-service -- --control-port 47801
```

Control requests use framed IPC v3 envelopes on the `control.rpc` channel.
Example request envelope:

```json
{
  "v": 3,
  "channel": "control.rpc",
  "msg_type": "request",
  "seq": 1,
  "request_id": "1",
  "sent_at_us": 1739596800000000,
  "payload": {
    "request_id": "1",
    "command": {"type": "snapshot"}
  }
}
```

`runtime.events` supports resume/replay with bounded retention (`10_000` events or `120s`) and
structured replay miss signaling (`requested_seq`, `replay_window_start_seq`,
`replay_window_end_seq`). Clients should treat `state="replay_miss"` as a required resync trigger.

## Control CLI

You can get runtime status and toggle action output **without opening the Hub** by calling the
service binary with the `control` subcommand. Developers can script control (e.g. snapshot,
set-output-enabled) via this CLI.

**Commands:**

- `neurohid-service control snapshot [--endpoint ADDR]` — Print runtime status:
  `device_connected`, `decoder_ready`, `output_enabled`, `pipeline_integrity_degraded`,
  `integrity_issue_count`.
- `neurohid-service control set-output-enabled <true|false> [--endpoint ADDR]` — Enable or
  disable action output (e.g. HID) while the runtime is running.

If `--endpoint` is omitted, the default is `127.0.0.1:47384` (same as the default control server
port when the service is started without config). The control request envelope and payload
format are described in [protocol-and-api](protocol-and-api.md).

## Observability and Tracing

Runtime binaries (`neurohid`, `neurohid-service`) emit structured `tracing` logs with low-overhead
defaults.

- Default format: JSON (`NEUROHID_LOG_FORMAT=json`)
- Optional human-readable format: `NEUROHID_LOG_FORMAT=text`
- Filter levels use standard `RUST_LOG` (for example: `RUST_LOG=neurohid=debug`)

Hot-path traces include correlation identifiers such as `decision_id` and `stream_id` across stage
boundaries (signal -> decoder -> action -> IPC).

Observability sampling/rate limits are configurable via `service.observability` in `SystemConfig`
(global + per-component: `device`, `signal`, `decoder`, `action`, `ipc`, `control`).

- `sample_ratio` controls deterministic sampling for hot-path debug events
- `info_max_per_minute` bounds gated info summaries
- `debug_max_per_second` bounds gated debug emissions

## Detached Visualization Window

The Hub supports rendering visualization in a detached OS-level secondary window.

- Runtime toggle: command palette or runtime status controls
- Persisted UI config fields:
  - `ui.visualization_detached`
  - `ui.visualization_detached_pos`
  - `ui.visualization_detached_size`
- Fallback behavior: if native secondary viewport is unavailable, visualization
  remains embedded and Hub surfaces a non-fatal warning indicator.

## Integrity Degradation Operations

Pipeline integrity operates with a warn+degrade policy by default.

- Structured issue event key: `event="pipeline.integrity_issue"`
- Stage coverage: `device`, `signal`, `decoder`, `action`, `ipc`
- Stream-level integrity is degraded first where possible
- Pipeline-level degradation is raised when:
  - all EEG streams are impacted, or
  - repeated critical violations exceed threshold

Control snapshot indicators for operators:

- `pipeline_integrity_degraded` (`bool`)
- `integrity_issue_count` (`u64`)
- `stage_health_summary` (`Option<String>`)

Operational triage path:

1. Check Runtime panel integrity chips/counters in Hub.
2. Inspect `stage_health_summary` for stage-local issue concentration.
3. Correlate with structured logs filtered by `pipeline.integrity_issue`.
4. Adjust signal/device settings or restart bridge/task paths if issue rate persists.

## Validation Harness (V1 Matrix)

Use the built-in validation binary to run soak, latency, and boot-mode matrix checks:

```bash
# 24h soak with periodic forced bridge reconnects
cargo run -p neurohid --bin neurohid-validate -- soak --duration-secs 86400 --reconnect-interval-secs 120

# Full/fallback/degraded latency/resource comparison
cargo run -p neurohid --bin neurohid-validate -- latency-matrix --duration-secs-per-mode 120

# No-Python-bridge boot scenario matrix
cargo run -p neurohid --bin neurohid-validate -- boot-matrix --settle-secs 8
```

Use `--service-bin <path>` (or `NEUROHID_SERVICE_BIN`) when the validation binary cannot
auto-locate `neurohid-service`.

## CI/CD and Release

Workflow coverage includes:

- PR/push CI matrix (`ci.yml`)
- Branch policy enforcement (`branch-policy.yml`)
- Release checks on version tags (`release.yml`)
- Manual publish workflow for crates (`publish-crates.yml`)
- Python quality workflow (`python-quality.yml`)
