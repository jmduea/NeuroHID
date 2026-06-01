# neurohid-types

Core type definitions shared across all NeuroHID components. Every crate in the
NeuroHID workspace depends on `neurohid-types` for a single, consistent set of
domain models.

## Modules

| Module | Key types | Purpose |
|---|---|---|
| `action` | `MouseMovement`, `MouseButton`, `Key`, `Action` | HID action descriptors for mouse and keyboard emulation |
| `config` | `SystemConfig`, `ServiceConfig`, `SignalConfig`, `DeviceConfig`, `ActionConfig` | Hierarchical runtime configuration |
| `control` | `ControlCommand`, `ControlRequest`, `ControlResponse`, `ControlSnapshot`, `TrainerSnapshot` | Runtime control plane messages |
| `device` | `DeviceInfo`, `DeviceStatus`, `StreamInfo` | EEG device metadata and connection state |
| `error` | `PlatformError`, `StorageError`, … | Shared error types for the entire ecosystem |
| `event` | `StreamMarker` | Session-level event markers |
| `ipc` | `IpcEnvelope`, `RuntimeEvent`, … | IPC v3 protocol types and wire format |
| `learning` | Reward signals, reinforcement-learning types | Online learning feedback loop |
| `model` | Model metadata, candidate model types | ML model lifecycle descriptors |
| `observability` | `ObservabilityComponent`, `EmitGate`, event/stage constants | Structured telemetry helpers |
| `observation` | `CursorState`, `ScreenInfo` | OS-level observation snapshots |
| `profile` | `ProfileId`, `ProfileMetadata`, `CalibrationState` | User profile and calibration state |
| `reward` | Reward types | Reward channel primitives |
| `signal` | `Sample`, `FeatureVector`, `ChannelConfig` | Biosignal data representation |

## Usage

This crate is typically used as a dependency by other NeuroHID crates. End users
should use the `neurohid` facade crate.

```toml
[dependencies]
neurohid-types = { version = "0.1" }
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
