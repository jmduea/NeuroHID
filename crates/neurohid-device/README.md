# neurohid-device

Device abstraction layer for biosensor hardware in NeuroHID.

## Features

- Unified traits for device discovery, connection, and streaming
- LSL (Lab Streaming Layer) backend for real hardware (feature-gated)
- BrainFlow backend adapter with normalized board metadata (feature-gated)
- Serial backend for USB/UART adapters (`csv_line` and `binary_i16_le` framing)
- Mock backend for testing and development

## Usage

This crate is typically used as a dependency by `neurohid-core`. End users should use the `neurohid` facade crate with the `device` feature enabled.

```toml
[dependencies]
neurohid = { version = "0.1", features = ["device"] }
```

## Building without LSL

To build without Lab Streaming Layer support:

```bash
cargo build -p neurohid-device --no-default-features
```

To build with BrainFlow backend support:

```bash
cargo build -p neurohid-device --no-default-features --features brainflow
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
