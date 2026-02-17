# Deployment and Operations Guide

All commands below assume the repository root as the current working directory.

## Runtime Modes

- Interactive desktop mode via `neurohid` binary
- Headless service mode via `neurohid-service`
- Validation mode via `neurohid-validate`

## Service Operation

Typical operations (Windows service workflow supported by the binary):

```bash
cargo run --release -p neurohid --bin neurohid-service -- --service-command install
cargo run --release -p neurohid --bin neurohid-service -- --service-command start
cargo run --release -p neurohid --bin neurohid-service -- --service-command status
cargo run --release -p neurohid --bin neurohid-service -- --service-command stop
cargo run --release -p neurohid --bin neurohid-service -- --service-command uninstall
```

## Local Control Endpoint

Enable the local JSON control endpoint:

```bash
cargo run -p neurohid --bin neurohid-service -- --control-port 47801
```

On Linux/macOS, use TCP loopback transport for both control and ML bridge:

```toml
[service]
control_transport = "tcp_loopback"
control_port = 47385
ml_transport = "tcp_loopback"
ipc_port = 47384
```

Run Python bridge against the same transport/port:

```bash
uv run --directory python neurohid-ml bridge --transport tcp_loopback --port 47384
```

Control requests are line-delimited JSON matching `neurohid_types::control::ControlRequest`,
for example:

```json
{"request_id":"1","command":{"type":"snapshot"}}
```

## Release and Publish Workflows

Workflow coverage includes:

- PR/push CI matrix (`ci.yml`)
- Branch policy enforcement (`branch-policy.yml`)
- Release verification on version tags (`release.yml`, `v*` tags)
- Manual crates.io publish workflow (`publish-crates.yml`, requires workflow_dispatch `confirm=PUBLISH` on `main`)
- Python quality workflow (`python-quality.yml`)

## Operational Notes

- Logs support structured tracing output
- Coverage + architecture gates are part of automated quality checks
- Deployment is local-host/service-oriented; no default cloud deployment manifests detected

## See Also

- `README.md`
- `docs/development-guide.md`
- `docs/index.md`
