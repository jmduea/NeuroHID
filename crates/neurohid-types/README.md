# neurohid-types

Core type definitions shared across all NeuroHID components.

## Features

- Signal types (Sample, ChannelConfig) for biosignal data representation
- Action types (MouseAction, KeyAction) for HID emulation
- Device types (DeviceInfo, DeviceStatus) and profiles
- Shared error types for the entire NeuroHID ecosystem

## Usage

This crate is typically used as a dependency by other NeuroHID crates. End users should use the `neurohid-sdk` facade crate.

```toml
[dependencies]
neurohid-types = { version = "0.1" }
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
