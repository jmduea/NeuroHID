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

## Transport Configuration

Named-pipe transports are Windows-focused. On Linux/macOS, use TCP loopback for both control and
ML bridge endpoints:

```toml
[service]
control_transport = "tcp_loopback"
control_port = 47385
ml_transport = "tcp_loopback"
ipc_port = 47384
```

Run the Python bridge against the configured transport:

```bash
uv run --directory python neurohid-ml bridge --transport tcp_loopback --port 47384
```

## IPC and Bridge Mode

The runtime supports both simulation and real Python bridge operation.

- Default behavior: `service.ipc_simulation_enabled = true`
- Require real bridge: set `service.ipc_simulation_enabled = false`

## Local Control Endpoint

Optional control endpoint exposure:

```bash
cargo run --release -p neurohid --bin neurohid-service -- --control-port 47801
```

Control requests are line-delimited JSON with `neurohid_types::control::ControlRequest` shape.
Example:

```json
{"request_id":"1","command":{"type":"snapshot"}}
```

## Observability and Tracing

Runtime binaries (`neurohid`, `neurohid-service`) emit structured `tracing` logs with low-overhead
defaults.

- Default format: JSON (`NEUROHID_LOG_FORMAT=json`)
- Optional human-readable format: `NEUROHID_LOG_FORMAT=text`
- Filter levels use standard `RUST_LOG` (for example: `RUST_LOG=neurohid=debug`)

Hot-path traces include correlation identifiers such as `decision_id` and `stream_id` across stage
boundaries (signal -> decoder -> action -> IPC).

Observability sampling/rate limits are configurable via `service.observability` in `SystemConfig`
(global + per-component: `signal`, `decoder`, `action`, `ipc`, `control`).

- `sample_ratio` controls deterministic sampling for hot-path debug events
- `info_max_per_minute` bounds gated info summaries
- `debug_max_per_second` bounds gated debug emissions

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
