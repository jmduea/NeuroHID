# neurohid-core

NeuroHID core service library.

## Features

- Device connection and management orchestration
- Real-time signal processing pipeline integration
- IPC communication with the Python ML layer
- HID emission and action dispatching
- Background service runtime with async task coordination

## Usage

This crate is a library for the `neurohid` binary. End users should use the `neurohid-sdk` facade crate with the `core` feature enabled.

```toml
[dependencies]
neurohid-sdk = { version = "0.1", features = ["core"] }
```

To run the headless service:

```bash
cargo run -p neurohid -- --bin neurohid-service
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
