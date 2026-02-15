# Architecture: Rust Core (`rust-core`)

## Scope

This document covers the Rust workspace under `crates/` and the runtime/application binaries in
`crates/neurohid`.

## Architectural Style

- Service-centric backend runtime with modular crates
- Shared contract-first design (`neurohid-types`) used across runtime/UI/integration
- Desktop control surface via egui (`neurohid-hub` + binaries)

## Primary Crates and Roles

| Crate | Role |
|---|---|
| `neurohid-types` | Shared domain/config/control/telemetry types |
| `neurohid-core` | Task orchestration and core runtime pipeline |
| `neurohid-device` | Device abstraction backends and stream handling |
| `neurohid-signal` | Signal filtering/feature extraction pipeline |
| `neurohid-platform` | HID emission abstractions |
| `neurohid-storage` | Encrypted profile/model persistence |
| `neurohid-ipc` | Runtime bridge protocol and transport |
| `neurohid-hub` | GUI screens, layouts, and management UI |
| `neurohid-calibration` | Calibration game flows |
| `neurohid-sdk` | Consumer-facing SDK facade |

## Runtime Binaries

- `neurohid`: hub/desktop UX and management shell
- `neurohid-service`: long-running service process
- `neurohid-validate`: soak/latency/boot matrix verification tool

## Observability and Operations

- Structured tracing (`tracing`, `tracing-subscriber`)
- Runtime log formatting and filtering through environment/config
- Workflow automation via GitHub Actions (`ci.yml`, `release.yml`, `branch-policy.yml`)

## Reliability Boundaries

- Runtime designed to continue operation despite bridge interruption/failure
- Local transport and service command modes reduce dependency on external infrastructure
