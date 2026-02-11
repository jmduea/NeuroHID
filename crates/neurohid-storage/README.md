# neurohid-storage

Secure profile and configuration storage for NeuroHID.

## Features

- Platform keychain integration (Linux/macOS/Windows) for credential storage
- AES-GCM encryption for local data at rest
- TOML-based configuration management
- Profile versioning and migration support

## Usage

This crate is typically used as a dependency by `neurohid-core` and `neurohid-hub`. End users should use the `neurohid-sdk` facade crate with the `storage` feature enabled.

```toml
[dependencies]
neurohid-sdk = { version = "0.1", features = ["storage"] }
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
