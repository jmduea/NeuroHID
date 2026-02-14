# neurohid-calibration

Calibration games and first-run wizard for NeuroHID.

## Features

- Interactive calibration games (Grid Maze, Target Tracking) rendered in egui with Armas-backed control primitives
- Labeled EEG data collection for decoder training
- First-run wizard for device setup and profile creation
- Real-time visual feedback during calibration sessions
- Welcome/signal-check phases include progress visuals and explicit quality-state messaging (good/fair/low) before game launch

## Usage

This crate is typically used as a dependency by `neurohid-hub`. End users should use the `neurohid-sdk` facade crate with the `calibration` feature enabled.

```toml
[dependencies]
neurohid-sdk = { version = "0.1", features = ["calibration"] }
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
