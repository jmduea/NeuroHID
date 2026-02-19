# neurohid-ipc

IPC layer for communication between the Rust core service and the Python ML layer.

## Features

- IPC v3 JSON envelope protocol over local transport
- Named pipe (Windows) and loopback TCP (cross-platform) backends
- Async client/server architecture built on tokio
- Channel-routed broker with session management
- Type-safe message passing with serde serialization

## Usage

This crate is typically used as a dependency by `neurohid-core` and the Python ML layer. End users should use the `neurohid-sdk` facade crate with the `ipc` feature enabled.

```toml
[dependencies]
neurohid-sdk = { version = "0.1", features = ["ipc"] }
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
