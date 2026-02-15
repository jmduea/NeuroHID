# Source Tree Analysis

## Top-Level Structure

```text
neurohid/
├── Cargo.toml
├── crates/
│   ├── neurohid/
│   ├── neurohid-core/
│   ├── neurohid-types/
│   ├── neurohid-device/
│   ├── neurohid-signal/
│   ├── neurohid-platform/
│   ├── neurohid-storage/
│   ├── neurohid-ipc/
│   ├── neurohid-calibration/
│   ├── neurohid-hub/
│   └── neurohid-sdk/
├── python/
│   ├── pyproject.toml
│   ├── src/neurohid_ml/
│   ├── tests/
│   └── notebooks/
├── .github/workflows/
├── docs/
└── _bmad/
```

## Critical Folders and Purpose

- `crates/neurohid/src/bin/`: executable entry points (`neurohid`, `neurohid-service`, `neurohid-validate`)
- `crates/neurohid-core/src/`: orchestration runtime for service tasks
- `crates/neurohid-types/src/`: shared contracts for config/control/observations/actions
- `crates/neurohid-ipc/src/`: Rust side of bridge protocol and transport plumbing
- `crates/neurohid-hub/src/screens/`: operator UI screens (dashboard/devices/profiles/calibration/settings)
- `python/src/neurohid_ml/`: Python bridge, decoder, ErrP detection, trainer, CLI
- `python/tests/`: Python behavior/unit/integration-style tests
- `.github/workflows/`: CI, branch policy, release, publishing, architecture gate

## Integration-Relevant Paths

- Rust control protocol types: `crates/neurohid-types/src/control.rs`
- Runtime service config and observability: `crates/neurohid-types/src/config.rs`
- Python CLI control client and bridge: `python/src/neurohid_ml/`
- Root run/test guidance: `README.md`, `CONTRIBUTING.md`

## Entry Point Summary

- Main user-facing app: `crates/neurohid/src/bin/neurohid.rs`
- Headless service: `crates/neurohid/src/bin/neurohid-service.rs`
- Validation matrix binary: `crates/neurohid/src/bin/neurohid-validate.rs`
- Python executable: `neurohid-ml` from `python/pyproject.toml`
