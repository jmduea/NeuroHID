# Contributing to NeuroHID

Thank you for your interest in contributing to NeuroHID! This guide will help you get set up for development.

## Prerequisites

### Rust Toolchain

- Rust 1.75 or later (install via [rustup](https://rustup.rs/))
- The project uses the 2021 edition

### Python Environment

For the ML module in `python/`:

- uv

```bash
cd python
uv sync
```

### LSL (Lab Streaming Layer)

If you're working with `neurohid-device` or real hardware:

- **Linux**: `sudo apt install liblsl-dev`
- **macOS**: `brew install labstreaminglayer/tap/lsl`
- **Windows**: Download from [LSL releases](https://github.com/sccn/liblsl/releases)

To build without LSL: `cargo build -p neurohid-device --no-default-features`

## Building

```bash
# Build the entire workspace
cargo build --workspace

# Build a specific crate
cargo build -p neurohid-device

# Build in release mode
cargo build --release
```

## Running

```bash
# Run the GUI application
cargo run -p neurohid --bin neurohid

# Run the headless service
cargo run -p neurohid --bin neurohid-service

# Run with verbose logging
RUST_LOG=neurohid=debug cargo run -p neurohid --bin neurohid
```

### IPC Mode

The core supports both simulated IPC and the real Python bridge.

- Default behavior: `service.ipc_simulation_enabled = true`
- To require a real Python bridge: set `service.ipc_simulation_enabled = false`
- Start the Python bridge with: `uv run --directory python neurohid-ml bridge`

On Linux/macOS, set control and ML transports to `tcp_loopback` because named pipes are
Windows-only.

## Testing

```bash
# Run all tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p neurohid-signal

# Run with output
cargo test --workspace -- --nocapture
```

## Code Quality

```bash
# Lint
cargo clippy --workspace -- -D warnings

# Format
cargo fmt --check

# Generate docs
cargo doc --workspace --no-deps --open
```

## Project Structure

The workspace is organized into internal library crates and two published crates:

- **`neurohid`** — published binary crate (`cargo install neurohid`)
- **`neurohid-sdk`** — published facade library for Rust BCI developers
- **Internal crates** — `neurohid-types`, `neurohid-signal`, `neurohid-device`, etc.
- **External Emotiv crates** — `emotiv-cortex-v2` and `emotiv-cortex-cli` are maintained in `https://github.com/jmduea/emotiv-cortex-rs`

See the root [README.md](./README.md) for the full architecture overview.

## Pull Requests

1. Fork and create a feature branch
2. Make your changes
3. Ensure `cargo test --workspace` passes
4. Ensure `cargo clippy --workspace -- -D warnings` is clean
5. Run `cargo fmt`
6. Open a PR with a clear description

## Areas Where Help Is Appreciated

- Platform-specific testing (especially macOS)
- ErrP detection algorithm improvements
- Alternative device support (OpenBCI, Muse)
- Documentation and tutorials
- User experience design

## License

By contributing, you agree that your contributions will be dual-licensed under MIT and Apache-2.0.
