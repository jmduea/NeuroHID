# neurohid-validate

NeuroHID validation harness — soak, latency, and boot matrix checks for the runtime service.

## Usage

```bash
cargo run -p neurohid-validate -- soak --duration-secs 60
cargo run -p neurohid-validate -- latency-matrix
cargo run -p neurohid-validate -- boot-matrix
```

## License

Licensed under either of MIT or Apache-2.0, at your option.
