# neurohid-ipc

IPC layer for communication between the Rust core service and the Python ML layer.

## Features

- JSON-over-Unix-socket protocol for human-readable debugging
- Async client/server architecture built on tokio
- Type-safe message passing with serde serialization
- Bidirectional command/response flow

## Usage

This crate is typically used as a dependency by `neurohid-core` and the Python ML layer. End users should use the `neurohid-sdk` facade crate with the `ipc` feature enabled.

```toml
[dependencies]
neurohid-sdk = { version = "0.1", features = ["ipc"] }
```

## Protocol Encoding Gate

Run the JSON v2 vs protobuf benchmark gate:

```bash
cargo run -p neurohid-ipc --bin protocol_encoding_gate
```

The command prints encode/decode latency and payload size for both encodings,
then reports a gate decision (`KEEP_JSON_V2` or `MIGRATE_TO_PROTOBUF`).

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
