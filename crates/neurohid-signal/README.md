# neurohid-signal

Real-time biosignal processing pipeline for NeuroHID.

## Features

- Band-pass filtering for EEG/EMG/EOG signal conditioning
- FFT-based power spectral density estimation (Welch method)
- Feature extraction for BCI decoding (power bands, spectral features)

## Usage

This crate is typically used as a dependency by `neurohid-core`. End users should use the `neurohid-sdk` facade crate with the `signal` feature enabled.

```toml
[dependencies]
neurohid-sdk = { version = "0.1", features = ["signal"] }
```

## License

Licensed under either of

- MIT license ([LICENSE-MIT](../../LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.
