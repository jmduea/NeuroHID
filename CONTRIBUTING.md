# Contributing to NeuroHID

Thank you for your interest in contributing to NeuroHID! This guide will help you get set up for development.

## Prerequisites

### Rust Toolchain

- Rust 1.85 or later (install via [rustup](https://rustup.rs/))
- The project uses the 2024 edition

### Python Environment

For the ML module in `python/`:

- uv
- Always use `uv` to run Python commands. Do not use bare `python` commands.

```bash
cd python
uv sync
```

### LSL (Lab Streaming Layer)

If you're working with `neurohid-device` or real hardware:

- **Linux**: `sudo apt install liblsl-dev`
- **macOS**: `brew install labstreaminglayer/tap/lsl`
- **Windows**: Download from [LSL releases](https://github.com/sccn/liblsl/releases)

Workspace builds currently pin `lsl-sys` via `[patch.crates-io]` to a shared
git source (pinned `rev`) so Linux-compatible behavior is reproducible across
multiple applications and clean clones without local vendoring.

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

For Python ML tests and quality checks:

```bash
# Install Python dev tools (pytest, black, ruff, mypy)
uv sync --directory python --extra dev

# Canonical Python test command
uv run --project python pytest python/tests -q

# Canonical Python quality commands
uv run --project python ruff check python/src python/tests
uv run --project python black --check python/src python/tests
uv run --project python mypy python/src
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
Use [docs/crate-boundaries.md](./docs/crate-boundaries.md) as the canonical
"where should this code live?" reference.

## Pull Requests

1. Fork and create a feature branch
2. Make your changes
3. Ensure `cargo test --workspace` passes
4. Ensure `cargo clippy --workspace -- -D warnings` is clean
5. Run `cargo fmt`
6. For Python changes, run `uv run --project python pytest python/tests -q`
7. Open a PR with a clear description

### Branch Policy (Required)

- Do not push directly to `main`.
- All `main` updates must come from a pull request merge.
- CI enforces this via `.github/workflows/branch-policy.yml`.
- CI also enforces governance consistency (`.github/workflows/governance-integrity.yml`) and PR TDD evidence (`.github/workflows/tdd-governance.yml`).
- Repository admin setup checklist: `docs/automation/branch-protection-checklist.md`.
- Canonical policy source of truth: `.github/automation/policy-manifest.json`.

Local guardrails before push:

```bash
pwsh -File ./.github/scripts/verify-governance-setup.ps1
pwsh -File ./.github/scripts/pre-push-governance-checks.ps1 -RustScope focused
```

### Release Policy

- Tag pushes (`v*`) run pre-release verification only (`.github/workflows/release.yml`).
- crates.io publishing is manual-only through `.github/workflows/publish-crates.yml` and requires explicit operator confirmation.

## Areas Where Help Is Appreciated

- Platform-specific testing (especially macOS)
- ErrP detection algorithm improvements
- Alternative device support (OpenBCI, Muse)
- Documentation and tutorials
- User experience design

## License

By contributing, you agree that your contributions will be dual-licensed under MIT and Apache-2.0.
