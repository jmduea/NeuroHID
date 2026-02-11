# neurohid-hub

NeuroHID Hub GUI library.

## Features

- Unified egui application for device management
- Interactive calibration game launcher
- Profile management and configuration editing
- Service start/stop control with live status monitoring

## Usage

This crate is a library for the `neurohid` binary. End users should use the `neurohid-sdk` facade crate with the `hub` feature enabled.

```toml
[dependencies]
neurohid-sdk = { version = "0.1", features = ["hub"] }
```

To run the GUI:

```bash
cargo run -p neurohid
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
