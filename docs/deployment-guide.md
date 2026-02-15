# Deployment and Operations Guide

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

Optional local control endpoint can be enabled with a control port and local transport settings.

## CI/CD and Release

Workflow coverage includes:

- PR/push CI matrix (`ci.yml`)
- Branch policy enforcement (`branch-policy.yml`)
- Release checks on version tags (`release.yml`)
- Manual publish workflow for crates (`publish-crates.yml`)
- Python quality workflow (`python-quality.yml`)

## Operational Notes

- Logs support structured tracing output
- Coverage + architecture gates are part of automated quality checks
- Deployment is local-host/service-oriented; no default cloud deployment manifests detected
