# neurohid-platform

Cross-platform HID emulation abstractions for NeuroHID.

## Features

- Mouse movement, clicks, and scrolling via platform-native APIs
- Keyboard input simulation (key press, release, text input)
- Optional screen capture support for visual feedback
- Platform support: Windows (Win32), macOS (Core Graphics), Linux (enigo)

## Usage

This crate is typically used as a dependency by `neurohid-core`. End users should use the `neurohid` facade crate with the `platform` feature enabled.

```toml
[dependencies]
neurohid = { version = "0.1", features = ["platform"] }
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
