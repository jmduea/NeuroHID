# neurohid-sdk

Feature-gated SDK for [NeuroHID](https://github.com/jmduea/neurohid) — a brain-computer interface that transforms EEG devices into standard PC peripherals.

## Overview

`neurohid-sdk` is a thin facade crate that re-exports NeuroHID's internal libraries behind feature flags. Enable only what you need to keep compile times fast and dependency trees minimal.

## Features

| Feature | Crate | Description |
|---------|-------|-------------|
| `types` *(default)* | `neurohid-types` | Core type definitions — signals, actions, devices, profiles |
| `signal` | `neurohid-signal` | Real-time biosignal processing (filtering, FFT, feature extraction) |
| `device` | `neurohid-device` | Device abstraction layer for biosensor hardware |
| `device-lsl` | `neurohid-device` + LSL | Device layer with Lab Streaming Layer stream support |
| `platform` | `neurohid-platform` | Cross-platform HID emulation (mouse, keyboard) |
| `storage` | `neurohid-storage` | Encrypted profile and configuration storage |
| `ipc` | `neurohid-ipc` | IPC layer for Rust ↔ Python ML communication |
| `calibration` | `neurohid-calibration` | Calibration games and first-run wizard (egui) |
| `full` | *all of the above* | Everything enabled |

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
neurohid-sdk = { version = "0.1", features = ["device", "signal"] }
```

Then in your code:

```rust
use neurohid_sdk::types;
use neurohid_sdk::device::{DeviceProvider, Device};
use neurohid_sdk::signal;
```

## License

MIT OR Apache-2.0
