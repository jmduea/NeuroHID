# Contributing to NeuroHID

Thanks for your interest in contributing to NeuroHID.

## Start Here

- Project introduction and scope: [`README.md`](./README.md)
- Developer setup/build/test workflows: [`docs/development-guide.md`](./docs/development-guide.md)
- Deployment and runtime operations: [`docs/deployment-guide.md`](./docs/deployment-guide.md)
- Documentation index: [`docs/index.md`](./docs/index.md)

## Prerequisites

### Rust Toolchain

- Rust `1.85+` (install via [rustup](https://rustup.rs/))
- Workspace edition: 2024

### Python Environment

For `python/` workflows:

- `uv`
- Always use `uv` to run Python commands (no bare `python` invocations)

```bash
uv sync --directory python
```

### LSL (Lab Streaming Layer)

If you are working with `neurohid-device` or real hardware:

- Linux: `sudo apt install liblsl-dev`
- macOS: `brew install labstreaminglayer/tap/lsl`
- Windows: Download from [LSL releases](https://github.com/sccn/liblsl/releases)

Workspace builds pin `lsl-sys` via `[patch.crates-io]` to a shared git source (`rev`) for
reproducible behavior.

## Typical Contributor Workflow

1. Create a feature branch from `main`.
2. Implement scoped changes in the relevant crate/package/doc area.
3. Run local checks from [`docs/development-guide.md`](./docs/development-guide.md).
4. Update affected docs when behavior, contracts, or workflows change.
5. Open a pull request targeting `main`.

## Pull Request Requirements

At minimum, ensure these pass locally for changed scope:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
uv run --project python pytest python/tests -q
```

## Project Structure Pointers

- Workspace crate placement rules: [`docs/crate-boundaries.md`](./docs/crate-boundaries.md)
- Rust + Python architecture docs: [`docs/index.md`](./docs/index.md)
- Python package usage: [`python/README.md`](./python/README.md)

## Branch and Release Policy

- Do not push directly to `main`; all updates land via PR merge.
- CI enforces branch policy via `.github/workflows/branch-policy.yml`.
- Tag pushes (`v*`) run pre-release verification in `.github/workflows/release.yml`.
- crates.io publishing is manual via `.github/workflows/publish-crates.yml`.
- Required checks and branch-protection guidance live in
  [`docs/development-guide.md`](./docs/development-guide.md).

## Areas Where Help Is Appreciated

- Platform-specific testing (especially macOS)
- ErrP detection and decoder improvements
- Alternative device support (OpenBCI, Muse)
- Documentation and tutorials
- UX refinement for hub and calibration workflows

## License

By contributing, you agree that contributions are dual-licensed under MIT and Apache-2.0.
