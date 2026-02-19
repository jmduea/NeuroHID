# neurohid-core

NeuroHID core service library.

## Features

- Device connection and management orchestration
- Real-time signal processing pipeline integration
- IPC communication with the Python ML layer
- HID emission and action dispatching
- Background service runtime with async task coordination

## Usage

This crate is an internal library used by the `neurohid` binaries. Most application users should run `neurohid` or `neurohid-service` from the `neurohid` crate.

```toml
[dependencies]
neurohid-sdk = { version = "0.1" }
```

To run the headless service:

```bash
cargo run -p neurohid --bin neurohid-service
```

By default the core service starts without requiring a running Python bridge. Start the Python bridge separately to enable ML-assisted decoding.

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
